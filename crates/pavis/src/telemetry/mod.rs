//! Telemetry module: Observability (logs, metrics, traces).
//!
//! # Architectural Invariants
//!
//! 1. **Non-Blocking**: Telemetry operations must never block the request path. Use `try_send` or background tasks.
//! 2. **No Panic**: Telemetry failures (e.g., full buffers) should result in dropped data, not crashes.
//! 3. **Minimal Overhead**: The cost of disabled telemetry should be near zero.

use pavis_core::TelemetryConfig;
use std::sync::Arc;

pub mod access_log;

pub struct Telemetry {
    pub access_log: Arc<access_log::AccessLog>,
}

impl Telemetry {
    pub fn new(config: &TelemetryConfig) -> (Self, access_log::AccessLogWorker) {
        let (access_log, worker) = access_log::AccessLog::new(&config.access_log);
        (
            Self {
                access_log: Arc::new(access_log),
            },
            worker,
        )
    }
}
