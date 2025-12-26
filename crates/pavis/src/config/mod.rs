//! Configuration types for Pavis proxy.
//!
//! # Architectural Invariants
//!
//! 1. **Validation First**: Configuration must be validated before being used by the runtime.
//! 2. **Type Safety**: Use types (e.g., `ValidatedConfig`) to represent valid states and prevent invalid usage.
//! 3. **Immutability**: Runtime configuration is generally immutable; dynamic updates should replace the entire config or use specific dynamic components.
//!
//! Some fields are defined but not yet used - they are planned for future phases.
//! See doc/ROADMAP.md for implementation timeline.

// TODO: Remove this once all config fields are implemented
#![allow(dead_code)]

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::ops::Deref;

mod validation;

#[derive(Debug, Clone)]
pub struct ValidatedConfig(Config);

impl Deref for ValidatedConfig {
    type Target = Config;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

impl Config {
    pub fn validate(self) -> Result<ValidatedConfig> {
        validation::validate(&self)?;
        Ok(ValidatedConfig(self))
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MatchType {
    #[default]
    Prefix,
    Exact,
    Regex,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LoadBalancer {
    #[default]
    Random,
    RoundRobin,
}

/// HTTP version preference for upstream connections
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum HttpVersion {
    /// HTTP/1.1 only (default)
    #[default]
    #[serde(alias = "1", alias = "1.1", alias = "http1")]
    H1,
    /// HTTP/2 only
    #[serde(alias = "2", alias = "http2")]
    H2,
    /// Prefer HTTP/2, fallback to HTTP/1.1
    #[serde(alias = "auto")]
    H2H1,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub worker_threads: Option<usize>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

/// Access log destination configuration
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AccessLogConfig {
    /// Disabled
    False,
    /// Log to stdout (default)
    #[default]
    Stdout,
    /// Log to a file
    File(String),
}

impl<'de> Deserialize<'de> for AccessLogConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Bool(bool),
            String(String),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Bool(false) => Ok(AccessLogConfig::False),
            Helper::Bool(true) => Err(serde::de::Error::custom("access_log cannot be true")),
            Helper::String(s) => match s.as_str() {
                "false" => Ok(AccessLogConfig::False),
                "stdout" => Ok(AccessLogConfig::Stdout),
                path if !path.is_empty() => Ok(AccessLogConfig::File(path.to_string())),
                _ => Err(serde::de::Error::custom(
                    "access_log must be 'false', 'stdout', or a file path",
                )),
            },
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
pub struct TracingConfig {
    pub enabled: bool,
    pub provider: String,
    pub sampling_rate: f64,
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize, Clone)]
pub struct ConnectionPoolConfig {
    /// Idle timeout in seconds for pooled connections. Default: 60
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// Connection timeout in seconds. Default: 5
    #[serde(default = "default_connection_timeout_secs")]
    pub connection_timeout_secs: u64,
}

fn default_idle_timeout_secs() -> u64 {
    60
}

fn default_connection_timeout_secs() -> u64 {
    5
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: default_idle_timeout_secs(),
            connection_timeout_secs: default_connection_timeout_secs(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CircuitBreaker {
    pub max_connections: usize,
    pub max_pending_requests: usize,
    pub max_retries: usize,
}

// TODO: Implement health check scheduling and endpoint status tracking
#[derive(Debug, Deserialize, Clone)]
pub struct HealthCheck {
    pub path: String,
    // TODO: Parse duration string (e.g., "5s") into Duration
    pub interval: String,
    pub timeout: Option<String>,
    pub unhealthy_threshold: Option<usize>,
    pub healthy_threshold: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Endpoint {
    pub ip: String,
    pub port: u16,
    pub weight: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VirtualHost {
    pub host: String,
    pub paths: Vec<Route>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Route {
    #[serde(default)]
    pub match_type: MatchType,
    pub path: String,
    // TODO: Implement request timeout
    pub timeout_ms: Option<u64>,
    // TODO: Implement retry policy
    pub retry: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
    #[serde(skip)]
    pub compiled_regex: Option<regex::Regex>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RetryPolicy {
    pub attempts: usize,
    // Using serde_yaml::Value or similar for mixed types might be needed if retry_on contains strings and ints,
    // but typically retry_on are status codes or strings. Let's use a custom deserializer or just Value for now
    // to match the "500, 502, 'gateway_error'" mix.
    // For simplicity here, assuming user input is consistent or we use a more flexible type.
    // The example showed [500, 502, 503, "gateway_error"...].
    // Let's use serde_yaml::Value to be safe.
    pub retry_on: Vec<serde_yaml::Value>,
    pub per_try_timeout_ms: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HeaderOperations {
    pub add: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        let mut config_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        config_path.push("config.yaml");

        let config_content =
            std::fs::read_to_string(config_path).expect("Failed to read config file");
        let config: Config =
            serde_yaml::from_str(&config_content).expect("Failed to deserialize config");

        assert_eq!(config.server.listen_addr, "0.0.0.0:8080");
        assert_eq!(config.server.worker_threads, None);
        assert!(config.server.tls.is_none());

        assert_eq!(config.telemetry.level, Some("debug".to_string()));
        assert_eq!(config.telemetry.pingora, Some("warn".to_string()));

        assert_eq!(config.upstreams.len(), 2);
        assert_eq!(config.upstreams[0].name, "backend-v1");
        assert_eq!(config.upstreams[0].endpoints.len(), 1);
        assert_eq!(config.upstreams[0].endpoints[0].port, 8081);
        // Default http_version should be H1
        assert_eq!(config.upstreams[0].http_version, HttpVersion::H1);

        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.routes[0].host, "backend");
        assert_eq!(config.routes[0].paths.len(), 2);

        assert_eq!(config.routes[0].paths[0].match_type, MatchType::Prefix);
        assert_eq!(config.routes[0].paths[0].path, "/api/v1");
        assert_eq!(
            config.routes[0].paths[0].destinations[0].upstream,
            "backend-v1"
        );

        assert_eq!(config.routes[0].paths[1].match_type, MatchType::Prefix);
        assert_eq!(config.routes[0].paths[1].path, "/api/v2");
        assert_eq!(
            config.routes[0].paths[1].destinations[0].upstream,
            "backend-v2"
        );
    }

    #[test]
    fn test_http_version_deserialization() {
        // Test all http_version variants
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  level: "info"
upstreams:
  - name: "default"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
  - name: "http1-explicit"
    http_version: "h1"
    endpoints:
      - ip: "127.0.0.1"
        port: 8082
  - name: "http2-only"
    http_version: "h2"
    endpoints:
      - ip: "127.0.0.1"
        port: 8083
  - name: "http2-prefer"
    http_version: "h2h1"
    endpoints:
      - ip: "127.0.0.1"
        port: 8084
routes: []
"#;

        let config: Config = serde_yaml::from_str(yaml).expect("Failed to deserialize");

        assert_eq!(config.upstreams[0].http_version, HttpVersion::H1); // default
        assert_eq!(config.upstreams[1].http_version, HttpVersion::H1); // explicit h1
        assert_eq!(config.upstreams[2].http_version, HttpVersion::H2); // h2
        assert_eq!(config.upstreams[3].http_version, HttpVersion::H2H1); // h2h1
    }

    #[test]
    fn test_access_log_deserialization() {
        // Test default (stdout when not specified)
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  level: "info"
upstreams: []
routes: []
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("Failed to deserialize");
        assert_eq!(config.telemetry.access_log, AccessLogConfig::Stdout);

        // Test stdout explicitly
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: "stdout"
upstreams: []
routes: []
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("Failed to deserialize");
        assert_eq!(config.telemetry.access_log, AccessLogConfig::Stdout);

        // Test false to disable
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: "false"
upstreams: []
routes: []
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("Failed to deserialize");
        assert_eq!(config.telemetry.access_log, AccessLogConfig::False);

        // Test boolean false
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: false
upstreams: []
routes: []
"#;
        let config: Config =
            serde_yaml::from_str(yaml).expect("Failed to deserialize boolean false");
        assert_eq!(config.telemetry.access_log, AccessLogConfig::False);

        // Test file path
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: "/var/log/pavis/access.log"
upstreams: []
routes: []
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("Failed to deserialize");
        assert_eq!(
            config.telemetry.access_log,
            AccessLogConfig::File("/var/log/pavis/access.log".to_string())
        );
    }

    #[test]
    fn test_connection_pool_deserialization() {
        // Test default values
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  level: "info"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8080
routes: []
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("Failed to deserialize");
        assert_eq!(config.upstreams[0].connection_pool.idle_timeout_secs, 60);
        assert_eq!(
            config.upstreams[0].connection_pool.connection_timeout_secs,
            5
        );

        // Test custom values
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  level: "info"
upstreams:
  - name: "backend"
    connection_pool:
      idle_timeout_secs: 120
      connection_timeout_secs: 10
    endpoints:
      - ip: "127.0.0.1"
        port: 8080
routes: []
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("Failed to deserialize");
        assert_eq!(config.upstreams[0].connection_pool.idle_timeout_secs, 120);
        assert_eq!(
            config.upstreams[0].connection_pool.connection_timeout_secs,
            10
        );
    }

    #[test]
    fn test_tls_config_deserialization() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8443"
  tls:
    enabled: true
    cert_path: "/path/to/cert.pem"
    key_path: "/path/to/key.pem"
telemetry:
  level: "info"
upstreams: []
routes: []
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("Failed to deserialize");

        assert!(config.server.tls.is_some());
        let tls = config.server.tls.unwrap();
        assert!(tls.enabled);
        assert_eq!(tls.cert_path, Some("/path/to/cert.pem".to_string()));
        assert_eq!(tls.key_path, Some("/path/to/key.pem".to_string()));
    }

    #[test]
    fn test_empty_config_sections() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: false
upstreams: []
routes: []
"#;
        let config: Config =
            serde_yaml::from_str(yaml).expect("Failed to deserialize empty sections");
        assert!(config.upstreams.is_empty());
        assert!(config.routes.is_empty());
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config {
            server: ServerConfig {
                listen_addr: "0.0.0.0:8080".to_string(),
                worker_threads: None,
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::False,
                tracing: None,
            },
            upstreams: vec![],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Prefix,
                    path: "/".to_string(),
                    timeout_ms: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "non-existent".to_string(),
                        weight: 1,
                    }],
                    compiled_regex: None,
                }],
            }],
        };

        // Should fail because upstream doesn't exist
        assert!(config.clone().validate().is_err());

        // Fix upstream
        config.upstreams.push(Upstream {
            name: "non-existent".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![Endpoint {
                ip: "127.0.0.1".to_string(),
                port: 80,
                weight: Some(1),
            }],
        });
        assert!(config.clone().validate().is_ok());

        // Test invalid listen addr
        config.server.listen_addr = "invalid".to_string();
        assert!(config.validate().is_err());
    }
}
