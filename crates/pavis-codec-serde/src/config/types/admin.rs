use serde::{Deserialize, Serialize};

/// Shutdown configuration DTO (sparse, before defaults applied).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ShutdownConfig {
    /// Whether graceful shutdown is enabled. Default: true.
    pub enabled: Option<bool>,
    /// Drain timeout in milliseconds. Default: 30000 (30 seconds).
    pub drain_timeout_ms: Option<u32>,
}

/// Admin API configuration DTO (sparse, before defaults applied).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AdminConfig {
    /// Whether admin API is enabled. Default: false.
    pub enabled: Option<bool>,
    /// Address to bind admin API. Default: "127.0.0.1:9901".
    pub address: Option<String>,
}
