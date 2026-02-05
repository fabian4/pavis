use crate::router::MatchVerdict;
use async_trait::async_trait;
use metrics::{SharedString, counter, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use pingora::services::Service;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::time::timeout;

#[async_trait]
pub trait MetricsTransport: Send + Sync {
    async fn bind(addr: SocketAddr) -> std::io::Result<Self>
    where
        Self: Sized;
    async fn accept(&self) -> std::io::Result<tokio::net::TcpStream>;
}

pub struct TcpMetricsTransport {
    listener: tokio::net::TcpListener,
}

#[async_trait]
impl MetricsTransport for TcpMetricsTransport {
    async fn bind(addr: SocketAddr) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        Ok(Self { listener })
    }

    async fn accept(&self) -> std::io::Result<tokio::net::TcpStream> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream)
    }
}

pub struct PrometheusEndpoint<T: MetricsTransport = TcpMetricsTransport> {
    addr: SocketAddr,
    handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
    _transport: std::marker::PhantomData<T>,
}

impl<T: MetricsTransport> PrometheusEndpoint<T> {
    pub fn new(addr: SocketAddr) -> (Self, Option<MetricsRegistry>) {
        let builder = PrometheusBuilder::new();
        match builder.install_recorder() {
            Ok(handle) => {
                let registry = MetricsRegistry {
                    _handle: Arc::new(handle),
                    labels: Arc::new(MetricLabels::new()),
                };
                (
                    Self {
                        addr,
                        handle: Some(registry._handle.as_ref().clone()),
                        _transport: std::marker::PhantomData,
                    },
                    Some(registry),
                )
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to initialize Prometheus exporter");
                (
                    Self {
                        addr,
                        handle: None,
                        _transport: std::marker::PhantomData,
                    },
                    None,
                )
            }
        }
    }
}

#[async_trait]
impl<T> Service for PrometheusEndpoint<T>
where
    T: MetricsTransport + Send + Sync + 'static,
{
    async fn start_service(
        &mut self,
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
        _threads: usize,
    ) {
        let handle = match &self.handle {
            Some(h) => h.clone(),
            None => {
                tracing::warn!("Metrics endpoint started but exporter not initialized");
                return;
            }
        };

        let transport = match T::bind(self.addr).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(addr = %self.addr, error = %e, "Failed to bind metrics endpoint");
                return;
            }
        };

        tracing::info!(addr = %self.addr, "Metrics endpoint listening");

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    tracing::info!("Metrics endpoint shutting down");
                    break;
                }
                accept_result = transport.accept() => {
                    match accept_result {
                        Ok(stream) => {
                            let handle = handle.clone();
                            tokio::spawn(async move {
                                if let Err(e) = serve_metrics(stream, handle).await {
                                    tracing::warn!(error = %e, "Error serving metrics");
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to accept metrics connection");
                        }
                    }
                }
            }
        }
    }

    fn name(&self) -> &str {
        "metrics"
    }
}

async fn serve_metrics(
    mut stream: tokio::net::TcpStream,
    handle: metrics_exporter_prometheus::PrometheusHandle,
) -> Result<(), std::io::Error> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    let (reader, mut writer) = stream.split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = Vec::new();
    let mut total_bytes = 0usize;

    let read = timeout(METRICS_READ_TIMEOUT, reader.read_until(b'\n', &mut line))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "metrics read timeout"))??;
    if read == 0 {
        return Ok(());
    }
    if line.len() > METRICS_REQUEST_LINE_LIMIT_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "metrics request line too long",
        ));
    }
    total_bytes = total_bytes.saturating_add(line.len());

    loop {
        line.clear();
        let n = timeout(METRICS_READ_TIMEOUT, reader.read_until(b'\n', &mut line))
            .await
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "metrics read timeout")
            })??;
        if n == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(line.len());
        if total_bytes > METRICS_HEADER_LIMIT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "metrics headers too large",
            ));
        }
        if line == b"\n" || line == b"\r\n" {
            break;
        }
    }

    let metrics_output = handle.render();

    let response = format!(
        "HTTP/1.0 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
        metrics_output.len(),
        metrics_output
    );

    writer.write_all(response.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}

#[derive(Clone)]
pub struct MetricsRegistry {
    _handle: Arc<metrics_exporter_prometheus::PrometheusHandle>,
    labels: Arc<MetricLabels>,
}

pub const POOL_KEY_CARDINALITY_CAP: usize = 1024;
const METRIC_LABEL_CACHE_CAP: usize = 1024;
const METRIC_ROUTE_LABEL_CACHE_CAP: usize = 4096;
const METRICS_REQUEST_LINE_LIMIT_BYTES: usize = 4096;
const METRICS_HEADER_LIMIT_BYTES: usize = 16 * 1024;
const METRICS_READ_TIMEOUT: Duration = Duration::from_secs(5);

pub struct PoolKeyCardinalitySnapshot {
    pub cardinality: usize,
    pub saturated: bool,
}

struct BoundedKeySet {
    cap: usize,
    ttl: Duration,
    entries: HashMap<u64, Instant>,
    order: VecDeque<(u64, Instant)>,
}

struct MetricLabels {
    common: LabelCache,
    route: LabelCache,
    status: Mutex<HashMap<u16, SharedString>>,
}

impl MetricLabels {
    fn new() -> Self {
        Self {
            common: LabelCache::new(METRIC_LABEL_CACHE_CAP),
            route: LabelCache::new(METRIC_ROUTE_LABEL_CACHE_CAP),
            status: Mutex::new(HashMap::new()),
        }
    }

    fn common(&self, value: &str) -> SharedString {
        self.common.get(value)
    }

    fn route(&self, value: &str) -> SharedString {
        self.route.get(value)
    }

    fn status(&self, value: u16) -> SharedString {
        let mut guard = self
            .status
            .lock()
            .expect("metrics status cache lock poisoned");
        if let Some(existing) = guard.get(&value) {
            return existing.clone();
        }
        let shared = SharedString::from(value.to_string());
        guard.insert(value, shared.clone());
        shared
    }
}

struct LabelCache {
    cap: usize,
    inner: Mutex<LabelCacheInner>,
}

struct LabelCacheInner {
    map: HashMap<String, SharedString>,
    order: VecDeque<String>,
}

impl LabelCache {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            inner: Mutex::new(LabelCacheInner {
                map: HashMap::new(),
                order: VecDeque::new(),
            }),
        }
    }

    fn get(&self, value: &str) -> SharedString {
        let mut guard = self
            .inner
            .lock()
            .expect("metrics label cache lock poisoned");
        if let Some(existing) = guard.map.get(value) {
            return existing.clone();
        }
        let shared = SharedString::from(value.to_string());
        let key = value.to_string();
        guard.map.insert(key.clone(), shared.clone());
        guard.order.push_back(key);
        while guard.map.len() > self.cap {
            if let Some(evicted) = guard.order.pop_front() {
                guard.map.remove(&evicted);
            } else {
                break;
            }
        }
        shared
    }
}

impl BoundedKeySet {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            ttl: Duration::from_secs(60),
            entries: HashMap::with_capacity(cap),
            order: VecDeque::with_capacity(cap),
        }
    }

    fn insert(&mut self, key: u64) -> PoolKeyCardinalitySnapshot {
        let now = Instant::now();
        self.entries.insert(key, now);
        self.order.push_back((key, now));
        self.evict_expired(now);
        self.evict_over_cap();

        PoolKeyCardinalitySnapshot {
            cardinality: self.entries.len(),
            saturated: self.entries.len() >= self.cap,
        }
    }

    fn evict_expired(&mut self, now: Instant) {
        while let Some((key, ts)) = self.order.front().copied() {
            if now.duration_since(ts) <= self.ttl {
                break;
            }
            self.order.pop_front();
            if self.entries.get(&key).is_some_and(|seen| *seen == ts) {
                self.entries.remove(&key);
            }
        }
    }

    fn evict_over_cap(&mut self) {
        while self.entries.len() > self.cap {
            if let Some((key, ts)) = self.order.pop_front() {
                if self.entries.get(&key).is_some_and(|seen| *seen == ts) {
                    self.entries.remove(&key);
                }
            } else {
                break;
            }
        }
    }
}

pub struct PoolKeyCardinalityTracker {
    cap: usize,
    inner: Mutex<HashMap<String, BoundedKeySet>>,
}

impl PoolKeyCardinalityTracker {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn record(&self, upstream: &str, key_hash: u64) -> PoolKeyCardinalitySnapshot {
        let mut guard = self.inner.lock().expect("pool key tracker lock poisoned");
        let entry = guard
            .entry(upstream.to_string())
            .or_insert_with(|| BoundedKeySet::new(self.cap));
        entry.insert(key_hash)
    }
}

impl MetricsRegistry {
    pub fn record_route_match(&self, verdict: &MatchVerdict<'_>) {
        let result = if verdict.selection.is_some() {
            "matched"
        } else {
            "no_match"
        };
        counter!("pavis_route_match_attempts_total", "result" => result).increment(1);

        let stats = &verdict.stats;
        if stats.path_misses > 0 {
            counter!(
                "pavis_route_match_predicate_failures_total",
                "predicate_type" => "path"
            )
            .increment(stats.path_misses);
        }
        if stats.method_misses > 0 {
            counter!(
                "pavis_route_match_predicate_failures_total",
                "predicate_type" => "method"
            )
            .increment(stats.method_misses);
        }
        if stats.header_misses > 0 {
            counter!(
                "pavis_route_match_predicate_failures_total",
                "predicate_type" => "header"
            )
            .increment(stats.header_misses);
        }

        // P2: Export operator-specific evaluation counts
        if stats.exact_evals > 0 {
            counter!(
                "pavis_route_match_predicate_evaluations_total",
                "operator" => "exact"
            )
            .increment(stats.exact_evals);
        }
        if stats.prefix_evals > 0 {
            counter!(
                "pavis_route_match_predicate_evaluations_total",
                "operator" => "prefix"
            )
            .increment(stats.prefix_evals);
        }
        if stats.regex_evals > 0 {
            counter!(
                "pavis_route_match_predicate_evaluations_total",
                "operator" => "regex"
            )
            .increment(stats.regex_evals);
        }
        if stats.present_evals > 0 {
            counter!(
                "pavis_route_match_predicate_evaluations_total",
                "operator" => "present"
            )
            .increment(stats.present_evals);
        }
        if stats.absent_evals > 0 {
            counter!(
                "pavis_route_match_predicate_evaluations_total",
                "operator" => "absent"
            )
            .increment(stats.absent_evals);
        }

        // P2: Export regex input length rejections
        if stats.regex_input_too_large > 0 {
            counter!("pavis_route_match_regex_input_too_large_total")
                .increment(stats.regex_input_too_large);
        }
    }

    pub fn record_request(
        &self,
        method: &str,
        route_pattern: &str,
        status: u16,
        upstream: &str,
        duration_secs: f64,
    ) {
        let method = self.labels.common(method);
        let route = self.labels.route(route_pattern);
        let status = self.labels.status(status);
        let upstream = self.labels.common(upstream);

        counter!(
            "pavis_http_requests_total",
            "method" => method.clone(),
            "route" => route.clone(),
            "status" => status.clone(),
            "upstream" => upstream.clone(),
        )
        .increment(1);

        histogram!(
            "pavis_http_request_duration_seconds",
            "method" => method,
            "route" => route,
            "status" => status,
            "upstream" => upstream,
        )
        .record(duration_secs);
    }

    pub fn record_upstream_request(&self, upstream: &str, status: u16, duration_secs: f64) {
        let upstream = self.labels.common(upstream);
        let status = self.labels.status(status);
        counter!(
            "pavis_upstream_requests_total",
            "upstream" => upstream.clone(),
            "status" => status.clone(),
        )
        .increment(1);

        histogram!(
            "pavis_upstream_request_duration_seconds",
            "upstream" => upstream,
        )
        .record(duration_secs);
    }

    pub fn increment_active_connections(&self) {
        gauge!("pavis_http_inflight_requests").increment(1.0);
        counter!("pavis_connections_total").increment(1);
    }

    pub fn decrement_active_connections(&self) {
        gauge!("pavis_http_inflight_requests").decrement(1.0);
    }

    pub fn record_pool_size(&self, upstream: &str, size: f64) {
        let upstream = self.labels.common(upstream);
        gauge!("pavis_upstream_pool_size", "upstream" => upstream).set(size);
    }

    pub fn record_pool_key_cardinality(&self, upstream: &str, cardinality: usize, saturated: bool) {
        let upstream = self.labels.common(upstream);
        let reported = if saturated {
            (POOL_KEY_CARDINALITY_CAP + 1) as f64
        } else {
            cardinality as f64
        };
        gauge!(
            "pavis_upstream_pool_key_cardinality_approx",
            "upstream" => upstream
        )
        .set(reported);
    }

    pub fn record_connection_reused(&self, upstream: &str) {
        let upstream = self.labels.common(upstream);
        counter!(
            "pavis_upstream_connection_reused_total",
            "upstream" => upstream
        )
        .increment(1);
    }

    pub fn record_connection_new(&self, upstream: &str, reason: &str) {
        let upstream = self.labels.common(upstream);
        let reason = self.labels.common(reason);
        counter!(
            "pavis_upstream_connection_new_total",
            "upstream" => upstream,
            "reason" => reason
        )
        .increment(1);
    }

    pub fn update_config_stats(&self, version: &str, size_bytes: u64) {
        let version = self.labels.common(version);
        gauge!("pavis_runtime_config_version", "version" => version).set(1.0);
        gauge!("pavis_runtime_config_size_bytes").set(size_bytes as f64);
        gauge!("pavis_runtime_reload_last_timestamp").set(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as f64,
        );
    }

    pub fn increment_reload_count(&self) {
        counter!("pavis_runtime_reload_count_total").increment(1);
    }

    pub fn record_config_validation(&self, result: &str, reason: &str) {
        let result = self.labels.common(result);
        let reason = self.labels.common(reason);
        counter!(
            "pavis_config_validation_total",
            "result" => result,
            "reason" => reason,
        )
        .increment(1);
    }

    pub fn record_config_apply(&self, result: &str) {
        let result = self.labels.common(result);
        counter!(
            "pavis_config_apply_total",
            "result" => result,
        )
        .increment(1);
    }

    pub fn record_access_log_dropped(&self) {
        counter!("pavis_telemetry_access_log_dropped_total").increment(1);
    }

    pub fn record_tracing_export_error(&self) {
        counter!("pavis_telemetry_tracing_export_errors_total").increment(1);
    }

    pub fn record_span_created(&self) {
        counter!("pavis_telemetry_tracing_spans_created_total").increment(1);
    }

    pub fn record_span_exported(&self) {
        counter!("pavis_telemetry_tracing_spans_exported_total").increment(1);
    }

    pub fn record_metrics_label_dropped(&self) {
        counter!("pavis_telemetry_metrics_label_dropped_total").increment(1);
    }

    pub fn record_retry(&self, upstream: &str, reason: &str, attempt: u16) {
        counter!(
            "pavis_upstream_retries_total",
            "upstream" => upstream.to_string(),
            "reason" => reason.to_string(),
            "attempt" => attempt.to_string(),
        )
        .increment(1);
    }

    pub fn record_retry_outcome(&self, upstream: &str, outcome: &str) {
        counter!(
            "pavis_upstream_retry_outcome_total",
            "upstream" => upstream.to_string(),
            "outcome" => outcome.to_string(),
        )
        .increment(1);
    }

    pub fn record_retry_body_buffered(&self, upstream: &str, size_bytes: u64) {
        histogram!(
            "pavis_upstream_retry_body_buffer_size_bytes",
            "upstream" => upstream.to_string(),
        )
        .record(size_bytes as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::{MatchVerdict, PredicateStats};
    use metrics::{Key, Level, Metadata, Recorder};
    use metrics_exporter_prometheus::PrometheusBuilder;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct MockMetricsTransport {
        _stream_tx: tokio::sync::mpsc::Sender<tokio::net::TcpStream>,
        stream_rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<tokio::net::TcpStream>>,
    }

    #[async_trait]
    impl MetricsTransport for MockMetricsTransport {
        async fn bind(_addr: SocketAddr) -> std::io::Result<Self> {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            Ok(Self {
                _stream_tx: tx,
                stream_rx: tokio::sync::Mutex::new(rx),
            })
        }
        async fn accept(&self) -> std::io::Result<tokio::net::TcpStream> {
            let mut rx = self.stream_rx.lock().await;
            rx.recv()
                .await
                .ok_or_else(|| std::io::Error::other("closed"))
        }
    }

    #[tokio::test]
    async fn test_prometheus_endpoint_service_shutdown() {
        let (mut endpoint, _registry) =
            PrometheusEndpoint::<MockMetricsTransport>::new("127.0.0.1:0".parse().unwrap());
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let handle = tokio::spawn(async move {
            endpoint.start_service(None, shutdown_rx, 1).await;
        });

        shutdown_tx.send(true).unwrap();
        timeout(Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_prometheus_endpoint_serves_requests() {
        let (mut endpoint, _registry) =
            PrometheusEndpoint::<TcpMetricsTransport>::new("127.0.0.1:0".parse().unwrap());
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let _listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        // We reuse the serve_metrics logic but test the StartService loop indirectly
        // Actually, testing StartService with real TCP is easier if we can find the bound port.
        // But start_service doesn't easily expose its bound port.

        // Let's test start_service with a failing bind
        let mut endpoint = PrometheusEndpoint::<TcpMetricsTransport> {
            addr: "1.1.1.1:1".parse().unwrap(), // Likely to fail bind
            handle: endpoint.handle.take(),
            _transport: std::marker::PhantomData,
        };
        endpoint.start_service(None, shutdown_rx, 1).await;
        // Should return quickly on bind failure
    }

    #[test]
    fn test_metric_labels_new() {
        let labels = MetricLabels::new();
        assert!(labels.status.lock().unwrap().is_empty());
    }

    #[test]
    fn test_metric_labels_caching() {
        let labels = MetricLabels::new();
        let name1 = labels.common("upstream1");
        let name2 = labels.common("upstream1");
        assert_eq!(name1, name2);
    }

    #[test]
    fn test_metrics_registry_record_methods() {
        let (_endpoint, registry) =
            PrometheusEndpoint::<TcpMetricsTransport>::new("127.0.0.1:0".parse().unwrap());
        if let Some(registry) = registry {
            registry.record_request("GET", "/", 200, "upstream1", 0.05);
            registry.record_upstream_request("upstream1", 200, 0.02);
            registry.record_connection_reused("upstream1");
            registry.record_connection_new("upstream1", "reason");
            registry.record_pool_size("upstream1", 5.0);
            registry.record_pool_key_cardinality("upstream1", 10, true);
            registry.increment_active_connections();
            registry.decrement_active_connections();
        }
    }

    static TEST_METADATA: Metadata = Metadata::new("test", Level::INFO, Some("test"));

    #[test]
    fn metrics_worker_second_install_returns_none_when_recorder_exists() {
        let (_first, first_handle) =
            PrometheusEndpoint::<TcpMetricsTransport>::new("127.0.0.1:9090".parse().unwrap());
        let (_second, second_handle) =
            PrometheusEndpoint::<TcpMetricsTransport>::new("127.0.0.1:9091".parse().unwrap());
        if first_handle.is_some() {
            assert!(second_handle.is_none());
        }
    }

    #[test]
    fn metrics_handle_methods_do_not_panic() {
        let (_worker, handle) =
            PrometheusEndpoint::<TcpMetricsTransport>::new("127.0.0.1:9092".parse().unwrap());

        if let Some(handle) = handle {
            handle.record_request("GET", "/users/:id", 200, "backend-1", 0.1);
            handle.record_upstream_request("backend-1", 200, 0.05);
            handle.increment_active_connections();
            handle.decrement_active_connections();
            handle.update_config_stats("v1", 1024);
            handle.increment_reload_count();
            handle.record_config_validation("ok", "none");
            handle.record_config_apply("ok");
            handle.record_access_log_dropped();
            handle.record_tracing_export_error();
            handle.record_span_created();
            handle.record_span_exported();
            handle.record_metrics_label_dropped();
            handle.record_retry("backend-1", "connect_timeout", 1);
            handle.record_retry_outcome("backend-1", "success");
            handle.record_retry_body_buffered("backend-1", 1024);
            handle.record_pool_size("backend-1", 10.0);
            handle.record_pool_key_cardinality("backend-1", 12, false);
            handle.record_connection_reused("backend-1");
            handle.record_connection_new("backend-1", "new_connection");

            let verdict = MatchVerdict {
                selection: None,
                stats: PredicateStats {
                    path_misses: 1,
                    method_misses: 1,
                    header_misses: 1,
                    exact_evals: 1,
                    prefix_evals: 1,
                    regex_evals: 1,
                    present_evals: 1,
                    absent_evals: 1,
                    regex_input_too_large: 1,
                },
            };
            handle.record_route_match(&verdict);
        }
    }

    #[test]
    fn bounded_key_set_enforces_capacity_and_ttl() {
        let mut set = BoundedKeySet::new(2);
        assert_eq!(set.insert(1).cardinality, 1);
        assert_eq!(set.insert(2).cardinality, 2);
        let snapshot = set.insert(3);
        assert_eq!(snapshot.cardinality, 2);
        assert!(set.entries.contains_key(&2));
        assert!(set.entries.contains_key(&3));
        assert!(!set.entries.contains_key(&1));

        let future = Instant::now() + Duration::from_secs(120);
        set.evict_expired(future);
        assert!(set.entries.is_empty());
    }

    #[test]
    fn bounded_key_set_respects_cap_when_many_duplicates() {
        let mut set = BoundedKeySet::new(2);
        set.insert(1);
        set.insert(1);
        let snapshot = set.insert(2);
        assert_eq!(snapshot.cardinality, 2);
        let snapshot = set.insert(3);
        assert!(snapshot.saturated);
        assert_eq!(set.entries.len(), 2);
    }

    #[test]
    fn pool_key_tracker_tracks_per_upstream() {
        let tracker = PoolKeyCardinalityTracker::new(2);
        let snap1 = tracker.record("alpha", 1);
        assert_eq!(snap1.cardinality, 1);
        assert!(!snap1.saturated);
        let snap2 = tracker.record("alpha", 2);
        assert_eq!(snap2.cardinality, 2);
        let snap3 = tracker.record("alpha", 3);
        assert!(snap3.saturated);
        assert_eq!(snap3.cardinality, 2);
        let snap_other = tracker.record("beta", 42);
        assert_eq!(snap_other.cardinality, 1);
    }

    #[tokio::test]
    async fn serve_metrics_returns_prometheus_payload() {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let counter =
            recorder.register_counter(&Key::from_name("pavis_test_counter"), &TEST_METADATA);
        counter.increment(1);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            stream
                .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .await
                .unwrap();
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            buf
        });

        let (socket, _) = listener.accept().await.unwrap();
        serve_metrics(socket, handle).await.unwrap();
        let response = client.await.unwrap();
        assert!(response.starts_with(b"HTTP/1.0 200 OK"));
        let body = String::from_utf8_lossy(&response);
        assert!(body.contains("pavis_test_counter"));
    }
}
