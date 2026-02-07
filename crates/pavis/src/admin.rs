//! Admin API worker service.

use async_trait::async_trait;
use pavis_core::AdminConfig;
use pingora::services::Service;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::{Duration, Instant, timeout};

use crate::state::RuntimeStateHandle;

const ADMIN_REQUEST_LINE_LIMIT_BYTES: usize = 4096;
const ADMIN_READ_TIMEOUT: Duration = Duration::from_secs(5);

async fn read_request_line<R>(reader: &mut R) -> std::io::Result<Option<String>>
where
    R: AsyncBufReadExt + Unpin,
{
    let mut buf = Vec::new();
    let read = timeout(ADMIN_READ_TIMEOUT, reader.read_until(b'\n', &mut buf))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "admin read timeout"))??;
    if read == 0 {
        return Ok(None);
    }
    if buf.len() > ADMIN_REQUEST_LINE_LIMIT_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "admin request line too long",
        ));
    }
    let line = String::from_utf8_lossy(&buf);
    Ok(Some(line.trim_end_matches(['\r', '\n']).to_string()))
}

/// Admin API worker service.
///
/// Provides read-only operational endpoints:
/// - `GET /health` - Health status (always returns 200 OK)
/// - `GET /stats` - Runtime statistics (version, uptime, config counts)
pub struct AdminApiWorker {
    config: AdminConfig,
    state: Arc<RuntimeStateHandle>,
    start_time: Instant,
}

impl AdminApiWorker {
    /// Create a new admin API worker.
    pub fn new(config: AdminConfig, state: Arc<RuntimeStateHandle>) -> Self {
        Self {
            config,
            state,
            start_time: Instant::now(),
        }
    }

    /// Handle an incoming HTTP request.
    async fn handle_request(&self, method: &str, path: &str) -> String {
        match (method, path) {
            ("GET", "/health") => self.handle_health(),
            ("GET", "/stats") => self.handle_stats(),
            _ => self.handle_not_found(path),
        }
    }

    /// Handle GET /health endpoint.
    fn handle_health(&self) -> String {
        let body = r#"{"status":"healthy"}"#;
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
    }

    /// Handle GET /stats endpoint.
    fn handle_stats(&self) -> String {
        let state = self.state.load();
        let uptime_secs = self.start_time.elapsed().as_secs();

        // Count total path routes across all virtual hosts
        let total_routes: usize = state
            .config
            .routes
            .iter()
            .map(|vhost| vhost.paths.len())
            .sum();

        let body = format!(
            r#"{{"version":"{}","config_version":{},"uptime_seconds":{},"listeners":{},"upstreams":{},"routes":{}}}"#,
            env!("CARGO_PKG_VERSION"),
            match state.config_version {
                Some(version) => version.to_string(),
                None => "null".to_string(),
            },
            uptime_secs,
            state.config.listeners.len(),
            state.config.upstreams.len(),
            total_routes
        );

        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
    }

    /// Handle 404 Not Found.
    fn handle_not_found(&self, path: &str) -> String {
        let body = format!(r#"{{"error":"Not Found","path":"{}"}}"#, path);
        format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
    }

    /// Run the admin API server.
    async fn run_server(
        &self,
        addr: std::net::SocketAddr,
        mut shutdown: watch::Receiver<bool>,
    ) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        tracing::info!(addr = %addr, "Admin API listening");

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("Admin API shutting down");
                        break;
                    }
                }
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _peer_addr)) => {
                            let mut reader = BufReader::new(stream);
                            match read_request_line(&mut reader).await {
                                Ok(None) => continue, // Connection closed
                                Ok(Some(request_line)) => {
                                    // Parse request line: "GET /path HTTP/1.1"
                                    let parts: Vec<&str> = request_line.split_whitespace().collect();
                                    if parts.len() >= 2 {
                                        let method = parts[0];
                                        let path = parts[1];

                                        let response = self.handle_request(method, path).await;
                                        let mut stream = reader.into_inner();
                                        if let Err(e) = stream.write_all(response.as_bytes()).await {
                                            tracing::warn!(error = %e, "Failed to write admin API response");
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Failed to read admin API request");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to accept admin API connection");
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Service for AdminApiWorker {
    async fn start_service(
        &mut self,
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: watch::Receiver<bool>,
        _threads: usize,
    ) {
        match self.config {
            AdminConfig::Disabled => {
                tracing::debug!("Admin API is disabled");
                // Wait for shutdown signal and exit immediately
                let _ = shutdown.changed().await;
            }
            AdminConfig::Enabled { addr } => {
                if let Err(e) = self.run_server(addr, shutdown).await {
                    tracing::error!(error = %e, "Admin API server failed");
                }
            }
            #[allow(unreachable_patterns)]
            _ => {
                tracing::warn!("Unknown admin config, not starting admin API");
                // Wait for shutdown signal and exit immediately
                let _ = shutdown.changed().await;
            }
        }
    }

    fn name(&self) -> &str {
        "admin-api"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        AccessLogPolicy, ListenerName, LogLevel, Metrics, ServiceName, ShutdownPolicy, Telemetry,
        TlsConfig, TracingPolicy, WorkerCount,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn test_config() -> pavis_core::ValidatedRuntimeConfig {
        use pavis_core::RuntimeConfigBuilder;

        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            })
            .shutdown(ShutdownPolicy::Disabled)
            .admin(AdminConfig::Disabled)
            .add_listener(
                pavis_core::ListenerBuilder::new()
                    .name(ListenerName("default".to_string()))
                    .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
                    .workers(WorkerCount::Auto)
                    .tls(TlsConfig::Disabled)
                    .build()
                    .unwrap(),
            )
            .build()
            .unwrap();

        // SAFETY: Test config is valid
        unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) }
    }

    #[tokio::test]
    async fn health_endpoint_returns_200() {
        let config = test_config();
        let state = Arc::new(RuntimeStateHandle::new(
            crate::state::RuntimeState::from_config(&config).unwrap(),
        ));

        let worker = AdminApiWorker::new(AdminConfig::Disabled, state);
        let response = worker.handle_request("GET", "/health").await;

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.ends_with(r#"{"status":"healthy"}"#));
    }

    #[tokio::test]
    async fn stats_endpoint_returns_json() {
        let config = test_config();
        let state = Arc::new(RuntimeStateHandle::new(
            crate::state::RuntimeState::from_config(&config).unwrap(),
        ));

        let worker = AdminApiWorker::new(AdminConfig::Disabled, state.clone());
        let response = worker.handle_request("GET", "/stats").await;
        let runtime = state.load();

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("version"));
        assert!(response.contains("uptime_seconds"));
        assert!(response.contains(&format!(
            r#""listeners":{}"#,
            runtime.config.listeners.len()
        )));
        assert!(response.contains(&format!(r#""routes":{}"#, runtime.config.routes.len())));
        assert!(response.contains(&format!(
            r#""upstreams":{}"#,
            runtime.config.upstreams.len()
        )));
    }

    #[tokio::test]
    async fn unknown_path_returns_404() {
        let config = test_config();
        let state = Arc::new(RuntimeStateHandle::new(
            crate::state::RuntimeState::from_config(&config).unwrap(),
        ));

        let worker = AdminApiWorker::new(AdminConfig::Disabled, state);
        let response = worker.handle_request("GET", "/unknown").await;

        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
        assert!(response.contains(r#""error":"Not Found""#));
        assert!(response.contains(r#""path":"/unknown""#));
    }

    #[tokio::test]
    async fn run_server_serves_health_and_shuts_down() {
        let config = test_config();
        let state = Arc::new(RuntimeStateHandle::new(
            crate::state::RuntimeState::from_config(&config).unwrap(),
        ));

        let addr = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);
            addr
        };

        let (tx, rx) = watch::channel(false);
        let worker = AdminApiWorker::new(AdminConfig::Enabled { addr }, state);
        let server = tokio::spawn(async move {
            worker.run_server(addr, rx).await.expect("server run");
        });

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::time::{Duration, sleep};

        let mut attempts = 0;
        let mut stream = loop {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(stream) => break stream,
                Err(err) if attempts < 50 => {
                    attempts += 1;
                    sleep(Duration::from_millis(10)).await;
                    continue;
                }
                Err(err) => panic!("connect failed: {err}"),
            }
        };
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .expect("write");
        let mut buf = vec![0u8; 256];
        let read = stream.read(&mut buf).await.expect("read");
        let response = String::from_utf8_lossy(&buf[..read]);
        assert!(response.contains("200 OK"));

        tx.send(true).expect("shutdown signal");
        server.await.expect("server join");
    }

    #[tokio::test]
    async fn test_read_request_line_too_long() {
        let (mut client, server) = tokio::io::duplex(ADMIN_REQUEST_LINE_LIMIT_BYTES + 100);
        let mut reader = BufReader::new(server);

        tokio::spawn(async move {
            let large_line = vec![b'a'; ADMIN_REQUEST_LINE_LIMIT_BYTES + 1];
            client.write_all(&large_line).await.unwrap();
            client.write_all(b"\n").await.unwrap();
        });

        let res = read_request_line(&mut reader).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn test_read_request_line_eof() {
        let (client, server) = tokio::io::duplex(100);
        let mut reader = BufReader::new(server);

        tokio::spawn(async move {
            drop(client);
        });

        let res = read_request_line(&mut reader).await.unwrap();
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn stats_endpoint_with_version() {
        let config = test_config();
        let mut state_data = crate::state::RuntimeState::from_config(&config).unwrap();
        state_data.config_version = Some(pavis_core::ConfigVersion(
            std::num::NonZeroU64::new(42).unwrap(),
        ));
        let state = Arc::new(RuntimeStateHandle::new(state_data));

        let worker = AdminApiWorker::new(AdminConfig::Disabled, state);
        let response = worker.handle_request("GET", "/stats").await;
        assert!(response.contains(r#""config_version":42"#));
    }

    #[tokio::test]
    async fn test_admin_api_bind_fail() {
        let state = Arc::new(RuntimeStateHandle::new(
            crate::state::RuntimeState::from_config(&test_config()).unwrap(),
        ));
        // Use a likely-to-fail port (privileged port 1)
        let addr = "127.0.0.1:1".parse().unwrap();
        let (_tx, rx) = watch::channel(false);
        let mut worker = AdminApiWorker::new(AdminConfig::Enabled { addr }, state);

        // This should return an error when it tries to bind
        worker.start_service(None, rx, 1).await;
    }
}
