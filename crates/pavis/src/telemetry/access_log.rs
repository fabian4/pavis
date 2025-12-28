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
                    // Shutdown signal received
                    // Check if we should exit immediately or drain?
                    // Usually shutdown signal means "stop accepting new work".
                    // But for access log, we might want to drain?
                    // For now, let's just break.
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
                                AccessLogConfig::False => {}
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
        let enabled = *config != AccessLogConfig::False;

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

        let log_line = format!(
            "{} {} {} {} {} {} {} {} {} {}\n",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            method,
            host,
            path,
            status,
            upstream,
            response_time,
            bytes_sent,
            client_ip,
            request_id
        );

        // Non-blocking send (lossy if full)
        let _ = self.tx.try_send(log_line);
    }
}

#[cfg(test)]
mod tests {
    use super::AccessLog;
    use pavis_core::AccessLogConfig;

    #[test]
    fn access_log_disabled_for_false() {
        let (access_log, _worker) = AccessLog::new(&AccessLogConfig::False);
        assert!(!access_log.enabled);
    }

    #[test]
    fn access_log_enabled_for_stdout() {
        let (access_log, _worker) = AccessLog::new(&AccessLogConfig::Stdout);
        assert!(access_log.enabled);
    }
}
