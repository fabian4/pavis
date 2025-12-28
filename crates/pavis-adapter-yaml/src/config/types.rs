use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Deref;

use pavis_core::{AccessLogConfig, HttpVersion, LoadBalancer, MatchType};

use super::validation;

#[derive(Debug, Clone)]
pub struct ValidatedConfig(YamlConfig);

impl Deref for ValidatedConfig {
    type Target = YamlConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct YamlConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

impl YamlConfig {
    pub fn validate(mut self) -> Result<ValidatedConfig> {
        validation::validate(&mut self)?;
        Ok(ValidatedConfig(self))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub worker_threads: Option<usize>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TelemetryConfig {
    pub level: Option<String>,
    pub pingora: Option<String>,
    pub service_name: Option<String>,
    // TODO: Implement prometheus metrics endpoint
    pub prometheus_addr: Option<String>,
    /// Access log: "off" (default), "stdout", or file path
    #[serde(default)]
    pub access_log: AccessLogConfig,
    // TODO: Implement OpenTelemetry tracing
    pub tracing: Option<TracingConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TracingConfig {
    pub enabled: bool,
    pub provider: String,
    pub sampling_rate: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Upstream {
    pub name: String,
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
    // TODO: Implement circuit breaker logic
    pub circuit_breaker: Option<CircuitBreaker>,
    // TODO: Implement health check logic
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
    pub idle_timeout: std::time::Duration,
    /// Connection timeout. Default: 5s
    #[serde(default = "default_connection_timeout", with = "humantime_serde")]
    pub connection_timeout: std::time::Duration,
}

fn default_idle_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(60)
}

fn default_connection_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(5)
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

// TODO: Implement health check scheduling and endpoint status tracking
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HealthCheck {
    pub path: String,
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default, with = "humantime_serde")]
    pub timeout: Option<std::time::Duration>,
    pub healthy_threshold: usize,
    pub unhealthy_threshold: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Endpoint {
    pub ip: String,
    pub port: u16,
    pub weight: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VirtualHost {
    pub host: String,
    pub paths: Vec<Route>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Route {
    #[serde(default)]
    pub match_type: MatchType,
    pub path: String,
    // TODO: Implement request timeout
    #[serde(default, with = "humantime_serde")]
    pub timeout: Option<std::time::Duration>,
    // TODO: Implement retry policy
    pub retry: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
    #[serde(skip)]
    pub compiled_regex: Option<regex::Regex>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RetryPolicy {
    pub attempts: usize,
    pub retry_on: Vec<serde_json::Value>,
    #[serde(with = "humantime_serde")]
    pub per_try_timeout: std::time::Duration,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HeaderOperations {
    pub add: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}
