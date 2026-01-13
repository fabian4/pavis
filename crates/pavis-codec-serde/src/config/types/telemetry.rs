use serde::{Deserialize, Serialize};

use pavis_core::AccessLogPolicy;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TelemetryConfig {
    pub level: Option<String>,
    pub pingora: Option<String>,
    pub service_name: Option<String>,
    #[serde(rename = "metrics", alias = "prometheus_addr")]
    pub metrics: Option<String>,
    pub access_log: Option<AccessLogPolicy>,
    pub tracing: Option<TracingConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TracingConfig {
    pub provider: Option<String>,
    pub sampling: Option<u32>,
    pub endpoint: Option<String>,
}
