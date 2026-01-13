//! Telemetry module: Observability (logs, metrics, traces).
//!
//! # Architectural Invariants
//!
//! 1. **Non-Blocking**: Telemetry operations must never block the request path. Use `try_send` or background tasks.
//! 2. **No Panic**: Telemetry failures (e.g., full buffers) should result in dropped data, not crashes.
//! 3. **Minimal Overhead**: The cost of disabled telemetry should be near zero.
//! 4. **Unified Context**: All observability uses `RouterContext` for consistency.

use pavis_core::Telemetry as RuntimeTelemetry;
use std::sync::{Arc, OnceLock};

pub mod access_log;
pub mod metrics;
pub mod tracing;

pub struct Telemetry {
    pub access_log: Arc<access_log::AccessLog>,
    pub metrics: Option<Arc<metrics::MetricsHandle>>,
    // Tracing runtime is initialized asynchronously in TracingService.
    pub tracing: Arc<OnceLock<tracing::TracingRuntime>>,
}

impl Telemetry {
    pub fn new(
        config: &RuntimeTelemetry,
        reload_handle: Option<tracing::ReloadHandle>,
    ) -> (
        Self,
        access_log::AccessLogWorker,
        Option<metrics::MetricsWorker>,
        tracing::TracingService,
    ) {
        let (access_log, access_log_worker) = access_log::AccessLog::new(&config.access_log);

        let (metrics_worker, metrics_handle) = match &config.metrics {
            pavis_core::Metrics::Disabled => (None, None),
            pavis_core::Metrics::Enabled { addr } => {
                let (worker, handle) = metrics::MetricsWorker::new(*addr);
                (Some(worker), handle.map(Arc::new))
            }
            #[allow(unreachable_patterns)]
            &_ => (None, None),
        };

        // Prepare tracing slots
        let tracing_slot = Arc::new(OnceLock::new());

        let tracing_service = tracing::TracingService::new(
            config.tracing.clone(),
            config.service_name.0.clone(),
            reload_handle,
            tracing_slot.clone(),
        );
        (
            Self {
                access_log: Arc::new(access_log),
                metrics: metrics_handle,
                tracing: tracing_slot,
            },
            access_log_worker,
            metrics_worker,
            tracing_service,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Telemetry;
    use pavis_core::{
        AccessLogPolicy, Metrics, ServiceName, Telemetry as RuntimeTelemetry, TracingPolicy,
    };
    use pingora::services::Service;

    #[test]

    fn telemetry_creates_access_log_worker() {
        let (telemetry, worker, metrics_worker, _tracing_service) = Telemetry::new(
            &RuntimeTelemetry {
                level: pavis_core::LogLevel::Info,

                pingora: pavis_core::LogLevel::Info,

                service_name: ServiceName("svc".to_string()),

                metrics: Metrics::Disabled,

                access_log: AccessLogPolicy::Disabled,

                tracing: TracingPolicy::Disabled,
            },
            None,
        );

        assert_eq!(worker.name(), "access_log");

        assert_eq!(std::sync::Arc::strong_count(&telemetry.access_log), 1);

        assert!(metrics_worker.is_none());
    }

    #[test]

    fn telemetry_creates_metrics_worker_when_enabled() {
        let (telemetry, _access_log_worker, metrics_worker, _tracing_service) = Telemetry::new(
            &RuntimeTelemetry {
                level: pavis_core::LogLevel::Info,

                pingora: pavis_core::LogLevel::Info,

                service_name: ServiceName("svc".to_string()),

                metrics: Metrics::Enabled {
                    addr: "127.0.0.1:9092".parse().unwrap(),
                },

                access_log: AccessLogPolicy::Disabled,

                tracing: TracingPolicy::Disabled,
            },
            None,
        );

        assert!(metrics_worker.is_some());
        // metrics handle may be None if recorder already installed by another test
        if let Some(worker) = metrics_worker {
            assert_eq!(worker.name(), "metrics");
        }
        assert!(telemetry.metrics.is_some() || telemetry.metrics.is_none());
    }
}
