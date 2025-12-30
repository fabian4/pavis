use serde::{Deserialize, Serialize};

use pavis_core::AccessLogConfig;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TelemetryConfig {
    pub level: Option<String>,
    pub pingora: Option<String>,
    pub service_name: Option<String>,
    pub prometheus_addr: Option<String>,
    /// Access log: "stdout" (default), "off", or file path
    #[serde(default)]
    pub access_log: AccessLogConfig,
    pub tracing: Option<TracingConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TracingConfig {
    pub enabled: bool,
    pub provider: String,
    pub sampling_rate: f64,
}
