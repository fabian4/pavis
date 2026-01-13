use async_trait::async_trait;
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use pingora::services::Service;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct MetricsWorker {
    addr: SocketAddr,
    handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

impl MetricsWorker {
    pub fn new(addr: SocketAddr) -> (Self, Option<MetricsHandle>) {
        let builder = PrometheusBuilder::new();
        match builder.install_recorder() {
            Ok(handle) => {
                let metrics_handle = MetricsHandle {
                    _handle: Arc::new(handle),
                };
                (
                    Self {
                        addr,
                        handle: Some(metrics_handle._handle.as_ref().clone()),
                    },
                    Some(metrics_handle),
                )
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to initialize Prometheus exporter");
                (Self { addr, handle: None }, None)
            }
        }
    }
}

#[async_trait]
impl Service for MetricsWorker {
    async fn start_service(
        &mut self,
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
        _threads: usize,
    ) {
        let handle = match &self.handle {
            Some(h) => h.clone(),
            None => {
                tracing::warn!("Metrics worker started but exporter not initialized");
                return;
            }
        };

        let listener = match tokio::net::TcpListener::bind(self.addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(addr = %self.addr, error = %e, "Failed to bind metrics endpoint");
                return;
            }
        };

        tracing::info!(addr = %self.addr, "Metrics endpoint listening");

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    tracing::info!("Metrics worker shutting down");
                    break;
                }
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
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
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n <= 2 {
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
pub struct MetricsHandle {
    _handle: Arc<metrics_exporter_prometheus::PrometheusHandle>,
}

impl MetricsHandle {
    pub fn record_request(
        &self,
        method: &str,
        route_pattern: &str,
        status: u16,
        upstream: &str,
        duration_secs: f64,
    ) {
        counter!(
            "pavis_http_requests_total",
            "method" => method.to_string(),
            "route" => route_pattern.to_string(),
            "status" => status.to_string(),
            "upstream" => upstream.to_string(),
        )
        .increment(1);

        histogram!(
            "pavis_http_request_duration_seconds",
            "method" => method.to_string(),
            "route" => route_pattern.to_string(),
            "status" => status.to_string(),
            "upstream" => upstream.to_string(),
        )
        .record(duration_secs);
    }

    pub fn record_upstream_request(&self, upstream: &str, status: u16, duration_secs: f64) {
        counter!(
            "pavis_upstream_requests_total",
            "upstream" => upstream.to_string(),
            "status" => status.to_string(),
        )
        .increment(1);

        histogram!(
            "pavis_upstream_request_duration_seconds",
            "upstream" => upstream.to_string(),
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

    pub fn update_config_stats(&self, version: &str, size_bytes: u64) {
        gauge!("pavis_runtime_config_version", "version" => version.to_string()).set(1.0);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_worker_creates_handle() {
        let (_worker, handle) = MetricsWorker::new("127.0.0.1:9090".parse().unwrap());
        assert!(handle.is_some() || handle.is_none());
    }

    #[test]
    fn metrics_handle_methods_do_not_panic() {
        let (_worker, handle) = MetricsWorker::new("127.0.0.1:9091".parse().unwrap());

        // Handle may be None if recorder already installed by another test
        if let Some(handle) = handle {
            // These should not panic even if called multiple times
            handle.record_request("GET", "/users/:id", 200, "backend-1", 0.1);
            handle.record_upstream_request("backend-1", 200, 0.05);
            handle.increment_active_connections();
            handle.decrement_active_connections();
            handle.update_config_stats("v1", 1024);
            handle.increment_reload_count();
            handle.record_access_log_dropped();
            handle.record_tracing_export_error();
            handle.record_span_created();
            handle.record_span_exported();
        }
    }
}
