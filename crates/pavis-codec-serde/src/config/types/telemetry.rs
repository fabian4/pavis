use serde::{Deserialize, Serialize};

use pavis_core::AccessLogPolicy;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TelemetryConfig {
    pub level: Option<String>,
    pub pingora: Option<String>,
    pub service_name: Option<String>,
    #[serde(rename = "metrics", alias = "prometheus_addr")]
    pub metrics: Option<String>,
    /// Access log: "stdout" (default), "off", or file path
    #[serde(default)]
    pub access_log: AccessLogPolicy,
    pub tracing: Option<TracingConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TracingConfig {
    pub provider: Option<String>,
    pub sampling: Option<u32>,
}
