use metrics::{counter, gauge};
use pavis_core::Upstream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

const METRIC_POOL_QUEUE_CAPACITY: &str = "pavis_upstream_pool_queue_capacity";
const METRIC_POOL_QUEUE_DEPTH: &str = "pavis_upstream_pool_queue_depth";
const METRIC_POOL_SIZE: &str = "pavis_upstream_pool_size";
const METRIC_POOL_REJECTIONS: &str = "pavis_upstream_pool_rejections_total";
const REASON_QUEUE_FULL: &str = "queue_full";
const REASON_QUEUE_TIMEOUT: &str = "queue_timeout";

#[derive(Debug)]
pub enum PoolRejection {
    QueueFull,
    QueueTimeout,
    Closed,
}

#[derive(Debug)]
pub struct PoolController {
    upstream: Arc<str>,
    limiter: Arc<PoolLimiter>,
}

#[derive(Debug)]
struct PoolLimiter {
    permits: Arc<Semaphore>,
    queue_capacity: u32,
    queue_timeout: Duration,
    queued: AtomicU32,
    active_conns: AtomicU32,
}

#[derive(Debug)]
pub struct PoolPermit {
    _permit: OwnedSemaphorePermit,
    limiter: Arc<PoolLimiter>,
    upstream: Arc<str>,
}

impl Drop for PoolPermit {
    fn drop(&mut self) {
        self.limiter.finish_pool_use(self.upstream.as_ref());
    }
}

impl PoolController {
    pub(crate) fn new(config: &Upstream) -> Self {
        let upstream = Arc::<str>::from(config.name.0.clone());
        record_queue_capacity_metric(upstream.as_ref(), config.pool.queue.capacity);
        record_queue_depth_metric(upstream.as_ref(), 0);
        record_pool_size_metric(upstream.as_ref(), 0);

        let limit = config.pool.max.0;
        let permits = Arc::new(Semaphore::new(limit.get() as usize));
        let limiter = Arc::new(PoolLimiter {
            permits,
            queue_capacity: config.pool.queue.capacity,
            queue_timeout: Duration::from_millis(config.pool.queue.timeout_ms as u64),
            queued: AtomicU32::new(0),
            active_conns: AtomicU32::new(0),
        });

        Self { upstream, limiter }
    }

    pub(crate) async fn acquire(&self) -> Result<PoolPermit, PoolRejection> {
        let permit = self.limiter.acquire(self.upstream.as_ref()).await?;
        Ok(PoolPermit {
            _permit: permit,
            limiter: self.limiter.clone(),
            upstream: self.upstream.clone(),
        })
    }

    fn record_rejection(upstream: &str, reason: &'static str) {
        let upstream = upstream.to_string();
        counter!(METRIC_POOL_REJECTIONS, "upstream" => upstream, "reason" => reason).increment(1);
    }
}

impl PoolLimiter {
    async fn acquire(&self, upstream: &str) -> Result<OwnedSemaphorePermit, PoolRejection> {
        if let Ok(permit) = self.permits.clone().try_acquire_owned() {
            self.start_pool_use(upstream);
            return Ok(permit);
        }

        if self.queue_capacity == 0 {
            PoolController::record_rejection(upstream, REASON_QUEUE_FULL);
            return Err(PoolRejection::QueueFull);
        }

        if self.queue_timeout.is_zero() {
            PoolController::record_rejection(upstream, REASON_QUEUE_TIMEOUT);
            return Err(PoolRejection::QueueTimeout);
        }

        let queued_before = self.queued.fetch_add(1, Ordering::SeqCst);
        if queued_before >= self.queue_capacity {
            self.queued.fetch_sub(1, Ordering::SeqCst);
            PoolController::record_rejection(upstream, REASON_QUEUE_FULL);
            return Err(PoolRejection::QueueFull);
        }
        record_queue_depth_metric(upstream, queued_before + 1);

        let result = timeout(self.queue_timeout, self.permits.clone().acquire_owned()).await;
        match result {
            Ok(Ok(permit)) => {
                self.finish_queue_wait(upstream);
                self.start_pool_use(upstream);
                Ok(permit)
            }
            Ok(Err(_)) => {
                self.finish_queue_wait(upstream);
                Err(PoolRejection::Closed)
            }
            Err(_) => {
                self.finish_queue_wait(upstream);
                PoolController::record_rejection(upstream, REASON_QUEUE_TIMEOUT);
                Err(PoolRejection::QueueTimeout)
            }
        }
    }

    fn start_pool_use(&self, upstream: &str) {
        let active = self.active_conns.fetch_add(1, Ordering::SeqCst) + 1;
        record_pool_size_metric(upstream, active);
    }

    fn finish_pool_use(&self, upstream: &str) {
        let active = self
            .active_conns
            .fetch_sub(1, Ordering::SeqCst)
            .saturating_sub(1);
        record_pool_size_metric(upstream, active);
    }

    fn finish_queue_wait(&self, upstream: &str) {
        let remaining = self.queued.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
        record_queue_depth_metric(upstream, remaining);
    }
}

fn record_queue_capacity_metric(upstream: &str, capacity: u32) {
    let upstream = upstream.to_string();
    gauge!(METRIC_POOL_QUEUE_CAPACITY, "upstream" => upstream).set(capacity as f64);
}

fn record_queue_depth_metric(upstream: &str, depth: u32) {
    let upstream = upstream.to_string();
    gauge!(METRIC_POOL_QUEUE_DEPTH, "upstream" => upstream).set(depth as f64);
}

fn record_pool_size_metric(upstream: &str, size: u32) {
    let upstream = upstream.to_string();
    gauge!(METRIC_POOL_SIZE, "upstream" => upstream).set(size as f64);
}
