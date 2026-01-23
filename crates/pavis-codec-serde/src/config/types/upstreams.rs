use serde::{Deserialize, Serialize};
use std::time::Duration;

use pavis_core::{Discovery, HttpVersion, LoadBalancer};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Upstream {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u16>,
    pub name: String,
    #[serde(alias = "discovery_type", skip_serializing_if = "Option::is_none")]
    pub discovery: Option<Discovery>,
    #[serde(
        rename = "balancer",
        alias = "load_balancer",
        alias = "lb",
        skip_serializing_if = "Option::is_none"
    )]
    pub balancer: Option<LoadBalancer>,
    /// HTTP version for upstream connections (h1, h2, h2h1). Default: h1
    #[serde(
        rename = "protocol",
        alias = "http_version",
        alias = "http",
        skip_serializing_if = "Option::is_none"
    )]
    pub protocol: Option<HttpVersion>,
    /// Connection pool settings
    #[serde(alias = "connection_pool", skip_serializing_if = "Option::is_none")]
    pub pool: Option<ConnectionPoolConfig>,
    /// TLS configuration for upstream connections
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<UpstreamTlsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circuit_breaker: Option<CircuitBreaker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlier_detection: Option<OutlierDetection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check: Option<HealthCheck>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpstreamTlsConfig {
    pub enabled: Option<bool>,
    pub verify_hostname: Option<bool>,
    pub verify_cert: Option<bool>,
    pub sni: Option<String>,
    #[serde(rename = "sni_mode", alias = "sniMode")]
    pub sni_mode: Option<SniMode>,
    #[serde(alias = "ca_bundle")]
    pub ca_bundle_path: Option<String>,
    pub cert: Option<ClientCertConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ClientCertConfig {
    pub cert_path: String,
    pub key_path: String,
    #[serde(default)]
    pub chain_path: Option<String>,
    #[serde(default)]
    pub chain_mode: Option<ClientCertChainMode>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClientCertChainMode {
    None,
    Embedded,
    File,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SniMode {
    Auto,
    Name,
    Disabled,
}

/// Connection pool configuration for upstream connections
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConnectionPoolConfig {
    /// Idle timeout for pooled connections. Default: 60s
    #[serde(default, with = "humantime_serde")]
    pub idle: Option<Duration>,
    /// Connection timeout. Default: 5s
    #[serde(default, with = "humantime_serde")]
    pub connect: Option<Duration>,
    /// Connection limit. Default: codec materializes a finite default.
    pub max: Option<i64>,
    /// Maximum number of requests allowed to wait for an upstream connection. Default: 0 (no queue).
    #[serde(default)]
    pub queue_capacity: Option<i64>,
    /// Maximum time (in milliseconds) a queued request may wait before being failed. Default: 0 (immediate failure).
    #[serde(default)]
    pub queue_timeout_ms: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CircuitBreaker {
    pub max_connections: usize,
    pub max_pending_requests: usize,
    #[serde(default)]
    pub max_retries: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OutlierDetection {
    pub consecutive_errors: usize,
    #[serde(with = "humantime_serde")]
    pub eject_duration: Duration,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HealthCheck {
    pub path: String,
    #[serde(with = "humantime_serde")]
    pub interval: Duration,
    #[serde(default, with = "humantime_serde")]
    pub timeout: Option<Duration>,
    #[serde(default = "default_health_threshold")]
    pub healthy_threshold: usize,
    #[serde(default = "default_health_threshold")]
    pub unhealthy_threshold: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Endpoint {
    #[serde(rename = "address", alias = "addr", alias = "ip")]
    pub address: String,
    pub port: u16,
    pub weight: Option<u32>,
}

fn default_health_threshold() -> usize {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_health_threshold() {
        assert_eq!(default_health_threshold(), 1);
    }
}
