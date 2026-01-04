use serde::{Deserialize, Serialize};
use std::time::Duration;

use pavis_core::{Discovery, HttpVersion, LoadBalancer};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Upstream {
    pub id: Option<u16>,
    pub name: String,
    #[serde(default, alias = "discovery_type")]
    pub discovery: Discovery,
    #[serde(default, rename = "balancer", alias = "load_balancer", alias = "lb")]
    pub balancer: LoadBalancer,
    /// HTTP version for upstream connections (h1, h2, h2h1). Default: h1
    #[serde(default, rename = "protocol", alias = "http_version", alias = "http")]
    pub protocol: HttpVersion,
    /// Connection pool settings
    #[serde(default, alias = "connection_pool")]
    pub pool: ConnectionPoolConfig,
    /// TLS configuration for upstream connections
    pub tls: Option<UpstreamTlsConfig>,
    pub circuit_breaker: Option<CircuitBreaker>,
    pub health_check: Option<HealthCheck>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpstreamTlsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub verify_hostname: Option<bool>,
    pub verify_cert: Option<bool>,
    pub sni: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Connection pool configuration for upstream connections
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ConnectionPoolConfig {
    /// Idle timeout for pooled connections. Default: 60s
    #[serde(default = "default_idle_timeout", with = "humantime_serde")]
    pub idle: Duration,
    /// Connection timeout. Default: 5s
    #[serde(default = "default_connection_timeout", with = "humantime_serde")]
    pub connect: Duration,
    /// Connection limit. Default: 0 (unlimited)
    #[serde(default)]
    pub max: Option<u32>,
}

fn default_idle_timeout() -> Duration {
    Duration::from_secs(60)
}

fn default_connection_timeout() -> Duration {
    Duration::from_secs(5)
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            idle: default_idle_timeout(),
            connect: default_connection_timeout(),
            max: None,
        }
    }
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
