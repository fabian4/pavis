use metrics::SharedString;
use metrics::{counter, gauge};
use pavis_core::Upstream;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
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
const DEFAULT_POOL_REJECTION_SAMPLE_RATE: u64 = 64;

static POOL_REJECTION_SAMPLE_RATE: OnceLock<u64> = OnceLock::new();
static POOL_REJECTION_COUNTER: AtomicU64 = AtomicU64::new(0);
static POOL_UPSTREAM_LABELS: OnceLock<Mutex<HashMap<String, SharedString>>> = OnceLock::new();

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
    upstream_label: SharedString,
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
        let upstream_label = label_upstream(upstream.as_ref());
        record_queue_capacity_metric(&upstream_label, config.pool.queue.capacity);
        record_queue_depth_metric(&upstream_label, 0);
        record_pool_size_metric(&upstream_label, 0);

        let limit = config.pool.max.0;
        let permits = Arc::new(Semaphore::new(limit.get() as usize));
        let limiter = Arc::new(PoolLimiter {
            permits,
            upstream_label: upstream_label.clone(),
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

    fn record_rejection(upstream: &SharedString, reason: &'static str) {
        let sample_rate = pool_rejection_sample_rate();
        if sample_rate == 1 {
            counter!(METRIC_POOL_REJECTIONS, "upstream" => upstream.clone(), "reason" => reason)
                .increment(1);
            return;
        }
        let count = POOL_REJECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
        if count.is_multiple_of(sample_rate) {
            // Approximate full count by scaling the sampled increment.
            counter!(METRIC_POOL_REJECTIONS, "upstream" => upstream.clone(), "reason" => reason)
                .increment(sample_rate);
        }
    }
}

impl PoolLimiter {
    async fn acquire(&self, upstream: &str) -> Result<OwnedSemaphorePermit, PoolRejection> {
        if let Ok(permit) = self.permits.clone().try_acquire_owned() {
            self.start_pool_use(upstream);
            return Ok(permit);
        }

        if self.queue_capacity == 0 {
            PoolController::record_rejection(&self.upstream_label, REASON_QUEUE_FULL);
            return Err(PoolRejection::QueueFull);
        }

        if self.queue_timeout.is_zero() {
            PoolController::record_rejection(&self.upstream_label, REASON_QUEUE_TIMEOUT);
            return Err(PoolRejection::QueueTimeout);
        }

        let queued_before = self.queued.fetch_add(1, Ordering::SeqCst);
        if queued_before >= self.queue_capacity {
            self.queued.fetch_sub(1, Ordering::SeqCst);
            PoolController::record_rejection(&self.upstream_label, REASON_QUEUE_FULL);
            return Err(PoolRejection::QueueFull);
        }
        record_queue_depth_metric(&self.upstream_label, queued_before + 1);

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
                PoolController::record_rejection(&self.upstream_label, REASON_QUEUE_TIMEOUT);
                Err(PoolRejection::QueueTimeout)
            }
        }
    }

    fn start_pool_use(&self, _upstream: &str) {
        let active = self.active_conns.fetch_add(1, Ordering::SeqCst) + 1;
        record_pool_size_metric(&self.upstream_label, active);
    }

    fn finish_pool_use(&self, _upstream: &str) {
        let active = self
            .active_conns
            .fetch_sub(1, Ordering::SeqCst)
            .saturating_sub(1);
        record_pool_size_metric(&self.upstream_label, active);
    }

    fn finish_queue_wait(&self, _upstream: &str) {
        let remaining = self.queued.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
        record_queue_depth_metric(&self.upstream_label, remaining);
    }
}

fn record_queue_capacity_metric(upstream: &SharedString, capacity: u32) {
    gauge!(METRIC_POOL_QUEUE_CAPACITY, "upstream" => upstream.clone()).set(capacity as f64);
}

fn record_queue_depth_metric(upstream: &SharedString, depth: u32) {
    gauge!(METRIC_POOL_QUEUE_DEPTH, "upstream" => upstream.clone()).set(depth as f64);
}

fn record_pool_size_metric(upstream: &SharedString, size: u32) {
    gauge!(METRIC_POOL_SIZE, "upstream" => upstream.clone()).set(size as f64);
}

fn pool_rejection_sample_rate() -> u64 {
    *POOL_REJECTION_SAMPLE_RATE.get_or_init(|| {
        match std::env::var("PAVIS_POOL_REJECTION_SAMPLE_RATE") {
            Ok(value) => value
                .parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_POOL_REJECTION_SAMPLE_RATE),
            Err(_) => DEFAULT_POOL_REJECTION_SAMPLE_RATE,
        }
    })
}

fn label_upstream(upstream: &str) -> SharedString {
    let cache = POOL_UPSTREAM_LABELS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().expect("pool label cache lock poisoned");
    if let Some(existing) = guard.get(upstream) {
        return existing.clone();
    }
    let shared = SharedString::from(upstream.to_string());
    guard.insert(upstream.to_string(), shared.clone());
    shared
}
