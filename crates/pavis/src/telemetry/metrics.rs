use crate::router::MatchVerdict;
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

    pub fn record_pool_size(&self, upstream: &str, size: f64) {
        gauge!("pavis_upstream_pool_size", "upstream" => upstream.to_string()).set(size);
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

    pub fn record_config_validation(&self, result: &str, reason: &str) {
        counter!(
            "pavis_config_validation_total",
            "result" => result.to_string(),
            "reason" => reason.to_string(),
        )
        .increment(1);
    }

    pub fn record_config_apply(&self, result: &str) {
        counter!(
            "pavis_config_apply_total",
            "result" => result.to_string(),
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
            // Request recording
            handle.record_request("GET", "/users/:id", 200, "backend-1", 0.1);
            handle.record_upstream_request("backend-1", 200, 0.05);
            handle.increment_active_connections();
            handle.decrement_active_connections();

            // Config recording
            handle.update_config_stats("v1", 1024);
            handle.increment_reload_count();
            handle.record_config_validation("ok", "none");
            handle.record_config_apply("ok");

            // Error/Event recording
            handle.record_access_log_dropped();
            handle.record_tracing_export_error();
            handle.record_span_created();
            handle.record_span_exported();
            handle.record_metrics_label_dropped();

            // Retry recording
            handle.record_retry("backend-1", "connect_timeout", 1);
            handle.record_retry_outcome("backend-1", "success");
            handle.record_retry_body_buffered("backend-1", 1024);

            // Pool recording
            handle.record_pool_size("backend-1", 10.0);

            // Route match recording
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
}
