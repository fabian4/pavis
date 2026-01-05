use async_trait::async_trait;
use pavis_core::AccessLogPolicy;
use pingora::protocols::l4::socket::SocketAddr;
use pingora::proxy::Session;
use pingora::services::Service;
use std::sync::Arc;
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
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
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
                                    #[allow(clippy::collapsible_if)]
                                    if let Some(w) = &mut file_writer {
                                        if let Err(e) = w.write_all(log_line.as_bytes()).await {
                                            eprintln!("Failed to write to access log: {}", e);
                                        }
                                    }
                                }
                                AccessLogPolicy::Disabled => {}
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

    pub async fn log(
        &self,
        session: &mut Session,
        upstream_name: Option<&str>,
        start_time: std::time::Instant,
    ) {
        if !self.enabled {
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

        let upstream = upstream_name.unwrap_or("-");
        let response_time = start_time.elapsed().as_millis();
        let client_ip = session.client_addr().cloned();
        let bytes_sent = session.body_bytes_sent();
        let request_id = req
            .headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-");

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
            request_id: request_id.to_string(),
        };

        // Non-blocking send (lossy if full)
        let _ = self.tx.try_send(entry);
    }
}

struct LogEntry {
    timestamp: chrono::DateTime<chrono::Utc>,
    method: http::Method,
    host: String,
    path: String,
    status: u16,
    upstream: String,
    response_time: u128,
    bytes_sent: usize,
    client_ip: Option<SocketAddr>,
    request_id: String,
}

fn format_log_line(entry: &LogEntry) -> String {
    let client_ip = entry
        .client_ip
        .as_ref()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "-".to_string());
    format!(
        "{} {} {} {} {} {} {} {} {} {}\n",
        entry.timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        entry.method.as_str(),
        entry.host,
        entry.path,
        entry.status,
        entry.upstream,
        entry.response_time,
        entry.bytes_sent,
        client_ip,
        entry.request_id
    )
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
            timestamp: chrono::Utc::now(),
            method: Method::GET,
            host: "example.com".to_string(),
            path: "/api".to_string(),
            status: 200,
            upstream: "backend-1".to_string(),
            response_time: 100,
            bytes_sent: 512,
            client_ip: Some("127.0.0.1:1234".parse().unwrap()),
            request_id: "req-123".to_string(),
        });
        assert!(line.contains("GET example.com /api 200 backend-1 100 512 127.0.0.1:1234 req-123"));
    }

    #[tokio::test]
    async fn test_access_log_file_write() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("pavis_access_log_{}.log", std::process::id()));
        let config = AccessLogPolicy::File(Path(path.to_string_lossy().to_string()));

        let (access_log, mut worker) = AccessLog::new(&config);

        // Inject a log manually
        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            method: Method::GET,
            host: "example.com".to_string(),
            path: "/api".to_string(),
            status: 200,
            upstream: "backend-1".to_string(),
            response_time: 100,
            bytes_sent: 512,
            client_ip: Some("127.0.0.1:1234".parse().unwrap()),
            request_id: "req-123".to_string(),
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
        let (access_log, mut worker) = AccessLog::new(&AccessLogPolicy::Stdout);
        let mut rx = worker.rx.take().expect("rx");

        let (mut client, server) = tokio::io::duplex(1024);
        client
            .write_all(b"GET /api HTTP/1.1\r\nHost: example.com\r\nx-request-id: req-1\r\n\r\n")
            .await
            .expect("write request");
        let mut session = Session::new_h1(Box::new(server));
        session.read_request().await.expect("read request");

        access_log
            .log(&mut session, Some("upstream-a"), std::time::Instant::now())
            .await;

        let line = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("log line");
        assert_eq!(line.method, "GET");
        assert_eq!(line.host, "example.com");
        assert_eq!(line.path, "/api");
        assert_eq!(line.upstream, "upstream-a");
        assert_eq!(line.request_id, "req-1");
    }
}
