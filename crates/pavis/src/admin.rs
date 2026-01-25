//! Admin API worker service.

use async_trait::async_trait;
use pavis_core::AdminConfig;
use pingora::services::Service;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::state::RuntimeStateHandle;

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
                            let mut request_line = String::new();

                            match reader.read_line(&mut request_line).await {
                                Ok(0) => continue, // Connection closed
                                Ok(_) => {
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

        assert!(response.contains("200 OK"));
        assert!(response.contains(r#"{"status":"healthy"}"#));
    }

    #[tokio::test]
    async fn stats_endpoint_returns_json() {
        let config = test_config();
        let state = Arc::new(RuntimeStateHandle::new(
            crate::state::RuntimeState::from_config(&config).unwrap(),
        ));

        let worker = AdminApiWorker::new(AdminConfig::Disabled, state);
        let response = worker.handle_request("GET", "/stats").await;

        assert!(response.contains("200 OK"));
        assert!(response.contains("version"));
        assert!(response.contains("uptime_seconds"));
        assert!(response.contains("listeners"));
    }

    #[tokio::test]
    async fn unknown_path_returns_404() {
        let config = test_config();
        let state = Arc::new(RuntimeStateHandle::new(
            crate::state::RuntimeState::from_config(&config).unwrap(),
        ));

        let worker = AdminApiWorker::new(AdminConfig::Disabled, state);
        let response = worker.handle_request("GET", "/unknown").await;

        assert!(response.contains("404 Not Found"));
        assert!(response.contains(r#""error":"Not Found""#));
    }
}
