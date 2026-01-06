use serde::{Deserialize, Serialize};
use std::time::Duration;

use pavis_core::{Discovery, HttpVersion, LoadBalancer};

#[derive(Debug, Deserialize, Serialize, Clone)]
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
    pub health_check: Option<HealthCheck>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpstreamTlsConfig {
    pub enabled: Option<bool>,
    pub verify_hostname: Option<bool>,
    pub verify_cert: Option<bool>,
    pub sni: Option<String>,
    pub cert: Option<ClientCertConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ClientCertConfig {
    pub cert_path: String,
    pub key_path: String,
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
    /// Connection limit. Default: 0 (unlimited)
    pub max: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CircuitBreaker {
    pub max_connections: usize,
    pub max_pending_requests: usize,
    pub max_retries: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HealthCheck {
    pub path: String,
    #[serde(with = "humantime_serde")]
    pub interval: Duration,
    #[serde(default, with = "humantime_serde")]
    pub timeout: Option<Duration>,
    pub healthy_threshold: usize,
    pub unhealthy_threshold: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Endpoint {
    #[serde(rename = "address", alias = "addr", alias = "ip")]
    pub address: String,
    pub port: u16,
    pub weight: Option<u32>,
}
