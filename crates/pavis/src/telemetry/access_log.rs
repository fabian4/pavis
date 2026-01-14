use async_trait::async_trait;
use pavis_core::AccessLogPolicy;
use pingora::protocols::l4::socket::SocketAddr;
use pingora::proxy::Session;
use pingora::services::Service;
use serde::Serialize;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;

pub struct AccessLog {
    tx: mpsc::Sender<LogEntry>,
    enabled: bool,
}

pub struct AccessLogWorker {
    rx: Option<mpsc::Receiver<LogEntry>>,
    config: AccessLogPolicy,
}

#[async_trait]
impl Service for AccessLogWorker {
    async fn start_service(
        &mut self,
        _fds: Option<std::sync::Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
        _threads: usize,
    ) {
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
                                    print!("{}", log_line);
                                }
                                AccessLogPolicy::File(_) => {
                                    if let Some(w) = &mut file_writer {
                                        if let Err(e) = w.write_all(log_line.as_bytes()).await {
                                            eprintln!("Failed to write to access log: {}", e);
                                        } else if let Err(e) = w.flush().await {
                                            eprintln!("Failed to flush access log: {}", e);
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
    }

    fn name(&self) -> &str {
        "access_log"
    }
}

impl AccessLog {
    pub fn new(config: &AccessLogPolicy) -> (Self, AccessLogWorker) {
        let (tx, rx) = mpsc::channel::<LogEntry>(4096);
        let enabled = *config != AccessLogPolicy::Disabled;

        let worker = AccessLogWorker {
            rx: Some(rx),
            config: config.clone(),
        };

        (Self { tx, enabled }, worker)
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
            request_id: ctx.req_id.clone(),
            rbac_denied: ctx.rbac_denied,
            route_pattern: route_pattern.to_string(),
            upstream_duration_ms,
        };

        // Non-blocking send (lossy if full)
        let _ = self.tx.try_send(entry);
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
    request_id: String,
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
            request_id: "req-123".to_string(),
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
            request_id: "req-123".to_string(),
            rbac_denied: false,
            route_pattern: "/api/*".to_string(),
            upstream_duration_ms: Some(50),
        };
        let expected = format_log_line(&entry);
        let _ = access_log.tx.try_send(entry);

        // Run worker briefly
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let worker_handle = tokio::spawn(async move {
            worker.start_service(None, shutdown_rx, 1).await;
        });

        // Let it process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Shutdown
        let _ = shutdown_tx.send(true);
        let _ = worker_handle.await;

        // Verify content
        let content = std::fs::read_to_string(&path).expect("read log file");
        assert_eq!(content, expected);

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn access_log_emits_entry_for_request() {
        use crate::proxy::context::{RoutePattern, RouterContext, TracingSpan, UpstreamTiming};
        use pavis_core::{HeadersPolicy, UpstreamName};
        use std::sync::Arc;

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
            upstream_name: Some(UpstreamName("upstream-a".to_string())),
            request_headers: std::sync::Arc::new(HeadersPolicy::Disabled),
            response_headers: std::sync::Arc::new(HeadersPolicy::Disabled),
            sni_override: None,
            start_time: std::time::Instant::now(),
            client_identity: None,
            rbac_denied: false,
            upstream_timing: UpstreamTiming::NotStarted,
            route_pattern: RoutePattern::Matched {
                pattern: Arc::from("/api/*"),
            },
            req_id: "req-1".to_string(),
            span: TracingSpan::Disabled,
            runtime_state: None,
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
        assert_eq!(line.request_id, "req-1");
        assert!(!line.rbac_denied);
        assert_eq!(line.route_pattern, "/api/*");
    }
}
