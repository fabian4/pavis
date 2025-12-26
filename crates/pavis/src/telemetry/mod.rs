//! Telemetry module: Observability (logs, metrics, traces).
//!
//! # Architectural Invariants
//!
//! 1. **Non-Blocking**: Telemetry operations must never block the request path. Use `try_send` or background tasks.
//! 2. **No Panic**: Telemetry failures (e.g., full buffers) should result in dropped data, not crashes.
//! 3. **Minimal Overhead**: The cost of disabled telemetry should be near zero.

use crate::config::TelemetryConfig;
use anyhow::Result;
use std::sync::Arc;

pub mod access_log;

pub struct Telemetry {
    pub access_log: Arc<access_log::AccessLog>,
}

impl Telemetry {
    pub async fn new(config: &TelemetryConfig) -> Result<Self> {
        let access_log = Arc::new(access_log::AccessLog::new(&config.access_log).await?);
        Ok(Self { access_log })
    }
}
