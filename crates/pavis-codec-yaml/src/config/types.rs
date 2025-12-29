use anyhow::{Context, Result as AnyResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;

use pavis_core::RuntimeConfig;
use pavis_core::{AccessLogConfig, HttpVersion, LoadBalancer, MatchType};

use super::validation;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct YamlConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

impl YamlConfig {
    pub fn parse_str(content: &str) -> AnyResult<Self> {
        serde_yaml::from_str(content).map_err(Into::into)
    }

    pub fn parse_bytes(bytes: &[u8]) -> AnyResult<Self> {
        let content = str::from_utf8(bytes).context("Config bytes must be UTF-8")?;
        Self::parse_str(content)
    }

    pub fn validate(&mut self) -> AnyResult<()> {
        validation::validate(self)
    }

    pub fn build(self) -> AnyResult<RuntimeConfig> {
        let mut config = self;
        validation::validate(&mut config)?;
        config.try_into()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
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

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
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

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct HeaderOperations {
    pub add: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}

#[cfg(test)]
mod tests {
    use super::YamlConfig;
    use pavis_core::{AccessLogConfig, HttpVersion, LoadBalancer};
    use std::time::Duration;

    #[test]
    fn parse_applies_defaults_for_upstream_and_telemetry() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    tls: {}
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
"#;

        let config = YamlConfig::parse_str(yaml).expect("parse yaml");
        let upstream = &config.upstreams[0];
        assert_eq!(upstream.load_balancer, LoadBalancer::Random);
        assert_eq!(upstream.http_version, HttpVersion::H1);
        assert_eq!(
            upstream.connection_pool.idle_timeout,
            Duration::from_secs(60)
        );
        assert_eq!(
            upstream.connection_pool.connection_timeout,
            Duration::from_secs(5)
        );
        let tls = upstream.tls.as_ref().expect("tls config");
        assert!(tls.enabled);
        assert_eq!(config.telemetry.access_log, AccessLogConfig::Stdout);
    }
}
