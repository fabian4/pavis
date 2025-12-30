use async_trait::async_trait;
use pavis_core::AccessLogConfig;
use pingora::proxy::Session;
use pingora::services::Service;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;

pub struct AccessLog {
    tx: mpsc::Sender<String>,
    enabled: bool,
}

pub struct AccessLogWorker {
    rx: Option<mpsc::Receiver<String>>,
    config: AccessLogConfig,
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

        let mut file_writer = if let AccessLogConfig::File(path) = &self.config {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                Ok(f) => Some(BufWriter::new(File::from_std(f))),
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
                        Some(log_line) => {
                            match &self.config {
                                AccessLogConfig::Stdout => {
                                    print!("{}", log_line);
                                }
                                AccessLogConfig::File(_) => {
                                    #[allow(clippy::collapsible_if)]
                                    if let Some(w) = &mut file_writer {
                                        if let Err(e) = w.write_all(log_line.as_bytes()).await {
                                            eprintln!("Failed to write to access log: {}", e);
                                        }
                                    }
                                }
                                AccessLogConfig::Disabled => {}
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
    pub fn new(config: &AccessLogConfig) -> (Self, AccessLogWorker) {
        let (tx, rx) = mpsc::channel::<String>(4096);
        let enabled = *config != AccessLogConfig::Disabled;

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
        let method = &req.method;
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
        let client_ip = session
            .client_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "-".to_string());
        let bytes_sent = session.body_bytes_sent();
        let request_id = req
            .headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-");

        let entry = LogEntry {
            method: method.as_str(),
            host,
            path,
            status,
            upstream,
            response_time,
            bytes_sent,
            client_ip: &client_ip,
            request_id,
        };
        let log_line = format_log_line(entry);

        // Non-blocking send (lossy if full)
        let _ = self.tx.try_send(log_line);
    }
}

struct LogEntry<'a> {
    method: &'a str,
    host: &'a str,
    path: &'a str,
    status: u16,
    upstream: &'a str,
    response_time: u128,
    bytes_sent: usize,
    client_ip: &'a str,
    request_id: &'a str,
}

fn format_log_line(entry: LogEntry<'_>) -> String {
    format!(
        "{} {} {} {} {} {} {} {} {} {}\n",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        entry.method,
        entry.host,
        entry.path,
        entry.status,
        entry.upstream,
        entry.response_time,
        entry.bytes_sent,
        entry.client_ip,
        entry.request_id
    )
}

#[cfg(test)]
mod tests {
    use super::{AccessLog, LogEntry, format_log_line};
    use pavis_core::AccessLogConfig;
    use pingora::services::Service;
    use tokio::sync::watch;

    #[test]
    fn access_log_disabled_for_false() {
        let (access_log, _worker) = AccessLog::new(&AccessLogConfig::Disabled);
        assert!(!access_log.enabled);
    }

    #[test]
    fn access_log_enabled_for_stdout() {
        let (access_log, _worker) = AccessLog::new(&AccessLogConfig::Stdout);
        assert!(access_log.enabled);
    }

    #[test]
    fn test_format_log_line() {
        let line = format_log_line(LogEntry {
            method: "GET",
            host: "example.com",
            path: "/api",
            status: 200,
            upstream: "backend-1",
            response_time: 100,
            bytes_sent: 512,
            client_ip: "127.0.0.1",
            request_id: "req-123",
        });
        assert!(line.contains("GET example.com /api 200 backend-1 100 512 127.0.0.1 req-123"));
    }

    #[tokio::test]
    async fn test_access_log_file_write() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("pavis_access_log_{}.log", std::process::id()));
        let config = AccessLogConfig::File(path.to_string_lossy().to_string());

        let (access_log, mut worker) = AccessLog::new(&config);

        // Inject a log manually
        let _ = access_log.tx.try_send("TEST_LOG_LINE\n".to_string());

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
        assert_eq!(content, "TEST_LOG_LINE\n");

        let _ = std::fs::remove_file(path);
    }
}
