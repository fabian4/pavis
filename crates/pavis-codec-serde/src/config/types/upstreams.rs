use serde::{Deserialize, Serialize};
use std::time::Duration;

use pavis_core::{DiscoveryType, HttpVersion, LoadBalancer};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Upstream {
    pub name: String,
    #[serde(default)]
    pub discovery_type: DiscoveryType,
    #[serde(default)]
    pub load_balancer: LoadBalancer,
    /// HTTP version for upstream connections (h1, h2, h2h1). Default: h1
    #[serde(default)]
    pub http_version: HttpVersion,
    /// Connection pool settings
    #[serde(default)]
    pub connection_pool: ConnectionPoolConfig,
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
    pub idle_timeout: Duration,
    /// Connection timeout. Default: 5s
    #[serde(default = "default_connection_timeout", with = "humantime_serde")]
    pub connection_timeout: Duration,
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
            idle_timeout: default_idle_timeout(),
            connection_timeout: default_connection_timeout(),
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
    #[serde(alias = "ip")]
    pub address: String,
    pub port: u16,
    pub weight: Option<u32>,
}
