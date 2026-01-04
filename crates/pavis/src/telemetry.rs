//! Telemetry module: Observability (logs, metrics, traces).
//!
//! # Architectural Invariants
//!
//! 1. **Non-Blocking**: Telemetry operations must never block the request path. Use `try_send` or background tasks.
//! 2. **No Panic**: Telemetry failures (e.g., full buffers) should result in dropped data, not crashes.
//! 3. **Minimal Overhead**: The cost of disabled telemetry should be near zero.

use pavis_core::Telemetry as RuntimeTelemetry;
use std::sync::Arc;

pub mod access_log;

pub struct Telemetry {
    pub access_log: Arc<access_log::AccessLog>,
}

impl Telemetry {
    pub fn new(config: &RuntimeTelemetry) -> (Self, access_log::AccessLogWorker) {
        let (access_log, worker) = access_log::AccessLog::new(&config.access_log);
        (
            Self {
                access_log: Arc::new(access_log),
            },
            worker,
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
        let (telemetry, worker) = Telemetry::new(&RuntimeTelemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName("svc".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: TracingPolicy::Disabled,
        });
        assert_eq!(worker.name(), "access_log");
        assert_eq!(std::sync::Arc::strong_count(&telemetry.access_log), 1);
    }
}
