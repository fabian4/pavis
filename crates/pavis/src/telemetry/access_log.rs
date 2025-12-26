use crate::config::AccessLogConfig;
use anyhow::Result;
use pingora::proxy::Session;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;

pub struct AccessLog {
    tx: mpsc::Sender<String>,
    enabled: bool,
}

impl AccessLog {
    pub async fn new(config: &AccessLogConfig) -> Result<Self> {
        let (tx, mut rx) = mpsc::channel::<String>(4096);
        let enabled = *config != AccessLogConfig::False;

        let config = config.clone();

        // Spawn background writer
        tokio::spawn(async move {
            let mut file_writer = if let AccessLogConfig::File(path) = &config {
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

            while let Some(log_line) = rx.recv().await {
                match &config {
                    AccessLogConfig::Stdout => {
                        // println! can block, but for now it's the standard way.
                        // Ideally we'd use tokio::io::stdout() but that requires locking too.
                        print!("{}", log_line);
                    }
                    AccessLogConfig::File(_) =>
                    {
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

            // Flush on exit
            if let Some(mut w) = file_writer {
                let _ = w.flush().await;
            }
        });

        Ok(Self { tx, enabled })
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
