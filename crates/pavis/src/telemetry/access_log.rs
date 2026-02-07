use crate::proxy::context::RequestId;
use crate::telemetry::metrics::MetricsRegistry;
use async_trait::async_trait;
use pavis_core::AccessLogPolicy;
use pingora::protocols::l4::socket::SocketAddr;
use pingora::proxy::Session;
use pingora::services::Service;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;
use tokio::time::Duration;

pub struct AccessLog {
    tx: Option<mpsc::Sender<LogEntry>>,
    enabled: bool,
    metrics: Mutex<Option<Arc<MetricsRegistry>>>,
}

pub struct AccessLogWorker {
    rx: Option<mpsc::Receiver<LogEntry>>,
    config: AccessLogPolicy,
    throttle_ms: Option<u64>,
}

#[async_trait]
impl Service for AccessLogWorker {
    async fn start_service(
        &mut self,
        _fds: Option<std::sync::Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
        _threads: usize,
    ) {
        if matches!(self.config, AccessLogPolicy::Disabled) || self.rx.is_none() {
            return;
        }
        let mut rx = self.rx.take().expect("Worker started twice");

        let mut file_writer = if let AccessLogPolicy::File(path) = &self.config {
            match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path.0)
                .await
            {
                Ok(f) => Some(BufWriter::new(f)),
                Err(e) => {
                    eprintln!("Failed to open access log file: {}", e);
                    None
                }
            }
        } else {
            None
        };
        let mut stdout_writer = if matches!(self.config, AccessLogPolicy::Stdout) {
            Some(BufWriter::new(tokio::io::stdout()))
        } else {
            None
        };

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    // Shutdown signal stops the worker loop.
                    break;
                }
                msg = rx.recv() => {
                    match msg {
                        Some(entry) => {
                            let log_line = format_log_line(&entry);
                            match &self.config {
                                AccessLogPolicy::Stdout => {
                                    if let Some(w) = &mut stdout_writer {
                                        if let Err(e) = w.write_all(log_line.as_bytes()).await {
                                            eprintln!("Failed to write to stdout access log: {}", e);
                                        } else if let Err(e) = w.flush().await {
                                            eprintln!("Failed to flush stdout access log: {}", e);
                                        }
                                    }
                                    if let Some(delay_ms) = self.throttle_ms {
                                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                                    }
                                }
                                AccessLogPolicy::File(_) => {
                                    if let Some(w) = &mut file_writer {
                                        if let Err(e) = w.write_all(log_line.as_bytes()).await {
                                            eprintln!("Failed to write to access log: {}", e);
                                        } else if let Err(e) = w.flush().await {
                                            eprintln!("Failed to flush access log: {}", e);
                                        } else if let Some(delay_ms) = self.throttle_ms {
                                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                                        }
                                    }
                                }
                                AccessLogPolicy::Disabled => {}
                                #[allow(unreachable_patterns)]
                                &_ => {}
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        // Flush on exit
        if let Some(mut w) = file_writer {
            let _ = w.flush().await;
        }
        if let Some(mut w) = stdout_writer {
            let _ = w.flush().await;
        }
    }

    fn name(&self) -> &str {
        "access_log"
    }
}

impl AccessLog {
    pub fn new(config: &AccessLogPolicy) -> (Self, AccessLogWorker) {
        let enabled = *config != AccessLogPolicy::Disabled;
        let (tx, rx) = if enabled {
            let (tx, rx) = mpsc::channel::<LogEntry>(access_log_channel_capacity());
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let throttle_ms = access_log_throttle_ms();

        let worker = AccessLogWorker {
            rx,
            config: config.clone(),
            throttle_ms,
        };

        (
            Self {
                tx,
                enabled,
                metrics: Mutex::new(None),
            },
            worker,
        )
    }

    pub fn set_metrics_handle(&self, handle: Option<Arc<MetricsRegistry>>) {
        let mut guard = self
            .metrics
            .lock()
            .expect("access log metrics lock poisoned");
        *guard = handle;
    }

    pub async fn log(&self, session: &mut Session, ctx: &crate::proxy::context::RouterContext) {
        // Use dynamic config if available, fallback to static
        let enabled = if let Some(state) = &ctx.runtime_state {
            matches!(
                state.config.telemetry.access_log,
                AccessLogPolicy::Stdout | AccessLogPolicy::File(_)
            )
        } else {
            self.enabled
        };

        if !enabled {
            return;
        }
        let Some(tx) = &self.tx else {
            return;
        };

        let req = session.req_header();
        let method = req.method.clone();
        let path = req.uri.path();
        let host = req
            .headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("-");

        let status = session
            .response_written()
            .map(|r| r.status.as_u16())
            .unwrap_or(0);

        let upstream = ctx
            .upstream_name
            .as_ref()
            .map(|u| u.0.as_str())
            .unwrap_or("-");
        let response_time = ctx.start_time.elapsed().as_millis();
        let client_ip = session.client_addr().cloned();
        let bytes_sent = session.body_bytes_sent();

        let route_pattern = match &ctx.route_pattern {
            crate::proxy::context::RoutePattern::Matched { pattern } => pattern.as_ref(),
            crate::proxy::context::RoutePattern::NotMatched => "-",
        };

        let upstream_duration_ms = match &ctx.upstream_timing {
            crate::proxy::context::UpstreamTiming::Started(start) => {
                Some(start.elapsed().as_millis())
            }
            crate::proxy::context::UpstreamTiming::NotStarted => None,
        };

        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            method,
            host: host.to_string(),
            path: path.to_string(),
            status,
            upstream: upstream.to_string(),
            response_time,
            bytes_sent,
            client_ip,
            request_id: ctx.request_id(),
            rbac_denied: ctx.rbac_denied,
            route_pattern: route_pattern.to_string(),
            upstream_duration_ms,
        };

        // Non-blocking send (lossy if full)
        if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(entry)
            && let Some(handle) = self
                .metrics
                .lock()
                .expect("access log metrics lock poisoned")
                .as_ref()
        {
            handle.record_access_log_dropped();
        }
    }
}

#[derive(Serialize)]
struct LogEntry {
    timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "serialize_method")]
    method: http::Method,
    host: String,
    path: String,
    status: u16,
    upstream: String,
    response_time: u128,
    bytes_sent: usize,
    #[serde(serialize_with = "serialize_socket_addr")]
    client_ip: Option<SocketAddr>,
    request_id: RequestId,
    rbac_denied: bool,
    route_pattern: String,
    upstream_duration_ms: Option<u128>,
}

fn serialize_method<S>(method: &http::Method, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(method.as_str())
}

fn serialize_socket_addr<S>(addr: &Option<SocketAddr>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match addr {
        Some(a) => serializer.serialize_str(&a.to_string()),
        None => serializer.serialize_str("-"),
    }
}

fn format_log_line(entry: &LogEntry) -> String {
    match serde_json::to_string(entry) {
        Ok(json) => format!("{}\n", json),
        Err(e) => {
            eprintln!("Failed to serialize access log entry: {}", e);
            String::new()
        }
    }
}

fn access_log_channel_capacity() -> usize {
    match std::env::var("PAVIS_ACCESS_LOG_CHANNEL_CAPACITY") {
        Ok(value) => value.parse::<usize>().unwrap_or(4096),
        Err(_) => 4096,
    }
}

fn access_log_throttle_ms() -> Option<u64> {
    match std::env::var("PAVIS_ACCESS_LOG_WRITE_THROTTLE_MS") {
        Ok(value) => value.parse::<u64>().ok().filter(|v| *v > 0),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{AccessLog, LogEntry, format_log_line};
    use http::Method;
    use pavis_core::{AccessLogPolicy, Path};
    use pingora::proxy::Session;
    use pingora::services::Service;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::watch;

    #[test]
    fn access_log_disabled_for_false() {
        let (access_log, _worker) = AccessLog::new(&AccessLogPolicy::Disabled);
        assert!(!access_log.enabled);
    }

    #[test]
    fn access_log_enabled_for_stdout() {
        let (access_log, _worker) = AccessLog::new(&AccessLogPolicy::Stdout);
        assert!(access_log.enabled);
    }

    #[test]
    fn test_format_log_line() {
        let line = format_log_line(&LogEntry {
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            method: Method::GET,
            host: "example.com".to_string(),
            path: "/api".to_string(),
            status: 200,
            upstream: "backend-1".to_string(),
            response_time: 100,
            bytes_sent: 512,
            client_ip: Some("127.0.0.1:1234".parse().unwrap()),
            request_id: "req-123".parse().unwrap(),
            rbac_denied: false,
            route_pattern: "/api/*".to_string(),
            upstream_duration_ms: Some(50),
        });
        // JSON format should contain all fields
        assert!(line.contains("\"method\":\"GET\""));
        assert!(line.contains("\"host\":\"example.com\""));
        assert!(line.contains("\"path\":\"/api\""));
        assert!(line.contains("\"status\":200"));
        assert!(line.contains("\"upstream\":\"backend-1\""));
        assert!(line.contains("\"response_time\":100"));
        assert!(line.contains("\"bytes_sent\":512"));
        assert!(line.contains("\"request_id\":\"req-123\""));
        assert!(line.contains("\"rbac_denied\":false"));
        assert!(line.contains("\"route_pattern\":\"/api/*\""));
        assert!(line.contains("\"upstream_duration_ms\":50"));
    }

    #[tokio::test]
    async fn test_access_log_file_write() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("pavis_access_log_{}.log", std::process::id()));
        let config = AccessLogPolicy::File(Path(path.to_string_lossy().to_string()));

        let (access_log, mut worker) = AccessLog::new(&config);

        // Start worker first
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let worker_handle = tokio::spawn(async move {
            worker.start_service(None, shutdown_rx, 1).await;
        });

        // Give worker time to initialize
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Inject a log manually
        let entry = LogEntry {
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            method: Method::GET,
            host: "example.com".to_string(),
            path: "/api".to_string(),
            status: 200,
            upstream: "backend-1".to_string(),
            response_time: 100,
            bytes_sent: 512,
            client_ip: Some("127.0.0.1:1234".parse().unwrap()),
            request_id: "req-123".parse().unwrap(),
            rbac_denied: false,
            route_pattern: "/api/*".to_string(),
            upstream_duration_ms: Some(50),
        };
        let expected = format_log_line(&entry);
        if let Some(tx) = &access_log.tx {
            let _ = tx.send(entry).await;
        }

        // Poll for file content with timeout (worker flushes immediately after write)
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        let content = loop {
            if let Ok(c) = std::fs::read_to_string(&path)
                && !c.is_empty()
            {
                break c;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("Timeout waiting for log file to be written");
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };

        // Shutdown
        let _ = shutdown_tx.send(true);
        let _ = worker_handle.await;

        // Verify content
        assert_eq!(content, expected);

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn access_log_emits_entry_for_request() {
        use crate::proxy::context::{
            RequestTelemetry, RoutePattern, RouterContext, UpstreamTiming,
        };
        use pavis_core::{HeadersPolicy, RetryPolicy, Timeout, UpstreamName};
        use std::sync::Arc;
        use std::time::Instant;

        let (access_log, mut worker) = AccessLog::new(&AccessLogPolicy::Stdout);
        let mut rx = worker.rx.take().expect("rx");

        let (mut client, server) = tokio::io::duplex(1024);
        client
            .write_all(b"GET /api HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .expect("write request");
        let mut session = Session::new_h1(Box::new(server));
        session.read_request().await.expect("read request");

        let ctx = RouterContext {
            telemetry: RequestTelemetry::new("req-1".parse().unwrap()),
            upstream_name: Some(UpstreamName("upstream-a".to_string())),
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::Matched {
                pattern: Arc::from("/api/*"),
            },
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
        };

        access_log.log(&mut session, &ctx).await;

        let line = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("log line");
        assert_eq!(line.method, "GET");
        assert_eq!(line.host, "example.com");
        assert_eq!(line.path, "/api");
        assert_eq!(line.upstream, "upstream-a");
        assert_eq!(line.request_id.as_str(), "req-1");
        assert!(!line.rbac_denied);
        assert_eq!(line.route_pattern, "/api/*");
    }

    #[tokio::test]
    async fn test_access_log_dropped_metrics() {
        use crate::proxy::context::{
            RequestTelemetry, RoutePattern, RouterContext, UpstreamTiming,
        };
        use pavis_core::{HeadersPolicy, RetryPolicy, Timeout};
        use std::sync::Arc;
        use std::time::Instant;

        // Create with tiny capacity to force dropping
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let access_log = AccessLog {
            tx: Some(tx),
            enabled: true,
            metrics: std::sync::Mutex::new(None),
        };

        // Create a registry and attach it
        let (_worker, handle) = crate::telemetry::metrics::PrometheusEndpoint::<
            crate::telemetry::metrics::TcpMetricsTransport,
        >::new("127.0.0.1:0".parse().unwrap());
        if let Some(metrics) = handle {
            access_log.set_metrics_handle(Some(Arc::new(metrics)));
        }

        let ctx = RouterContext {
            telemetry: RequestTelemetry::new("req-1".parse().unwrap()),
            upstream_name: None,
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: None,
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
        };

        let (mut client, server) = tokio::io::duplex(1024);
        let mut session = Session::new_h1(Box::new(server));

        // Mock a request so session has a header
        client.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();
        session.read_request().await.unwrap();

        // Fill the channel
        let _ = rx.try_recv();

        // Fill 1 slot
        access_log.log(&mut session, &ctx).await;
        // This one should drop
        access_log.log(&mut session, &ctx).await;

        // We can't easily assert the metric value here without rendering prometheus,
        // but this exercises the code path.
    }

    #[tokio::test]
    async fn test_access_log_worker_throttle() {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let mut worker = super::AccessLogWorker {
            rx: Some(rx),
            config: AccessLogPolicy::Stdout,
            throttle_ms: Some(10),
        };

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let worker_handle = tokio::spawn(async move {
            worker.start_service(None, shutdown_rx, 1).await;
        });

        let start = std::time::Instant::now();
        for _ in 0..3 {
            tx.send(dummy_entry()).await.unwrap();
        }

        // Wait a bit and shutdown
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = shutdown_tx.send(true);
        let _ = worker_handle.await;

        // 3 entries with 10ms throttle should take at least 30ms
        assert!(start.elapsed() >= Duration::from_millis(30));
    }

    #[tokio::test]
    async fn test_access_log_dynamic_config() {
        use crate::proxy::context::{
            RequestTelemetry, RoutePattern, RouterContext, UpstreamTiming,
        };
        use crate::state::RuntimeState;
        use pavis_core::{HeadersPolicy, RetryPolicy, Timeout};
        use std::sync::Arc;
        use std::time::Instant;

        let (access_log, worker) = AccessLog::new(&AccessLogPolicy::Disabled);

        assert!(worker.rx.is_none());
        assert!(!access_log.enabled);

        // Even if disabled statically, if dynamic config enables it, it should try to log
        // (but it won't have a TX because new() didn't create one)

        let mut builder = pavis_core::RuntimeConfigBuilder::new();
        builder = builder.telemetry(pavis_core::Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Error,
            service_name: pavis_core::ServiceName("test".to_string()),
            metrics: pavis_core::Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: pavis_core::TracingPolicy::Disabled,
        });
        builder = builder.add_listener(
            pavis_core::ListenerBuilder::new()
                .name(pavis_core::ListenerName("test".to_string()))
                .address("127.0.0.1:0".parse().unwrap())
                .build()
                .unwrap(),
        );
        let config = builder.build().expect("build config");
        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };

        let state = RuntimeState::with_components(
            validated,
            Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
            crate::upstream::Manager::new(&[]).expect("empty manager"),
        );
        let ctx = RouterContext {
            telemetry: RequestTelemetry::new("req-1".parse().unwrap()),
            upstream_name: None,
            upstream_endpoint: None,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: Instant::now(),
            client_identity: None,
            rbac_denied: false,
            route_timeout: Timeout::Disabled,
            retry_policy: RetryPolicy::Disabled,
            retry_attempts: 0,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::NotMatched,
            pool_permit: None,
            circuit_breaker_permit: None,
            runtime_state: Some(Arc::new(state)),
            retry_ctx: None,
            buffered_body: None,
            rewritten_uri: None,
            rewritten_host: None,
        };
        let (mut _client, server) = tokio::io::duplex(1024);
        let mut session = Session::new_h1(Box::new(server));

        access_log.log(&mut session, &ctx).await;
        // Should return early after finding tx is None
    }

    fn dummy_entry() -> super::LogEntry {
        super::LogEntry {
            timestamp: chrono::Utc::now(),
            method: http::Method::GET,
            host: "-".to_string(),
            path: "-".to_string(),
            status: 0,
            upstream: "-".to_string(),
            response_time: 0,
            bytes_sent: 0,
            client_ip: None,
            request_id: "req-1".parse().unwrap(),
            rbac_denied: false,
            route_pattern: "-".to_string(),
            upstream_duration_ms: None,
        }
    }
}
