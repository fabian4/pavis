//! Configuration types for Pavis proxy.
//!
//! Some fields are defined but not yet used - they are planned for future phases.
//! See ROADMAP.md for implementation timeline.

#![allow(dead_code)]

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
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

#[derive(Debug, Deserialize, Clone)]
pub struct TelemetryConfig {
    pub level: Option<String>,
    pub pingora: Option<String>,
    pub service_name: Option<String>,
    pub prometheus_addr: Option<String>,
    pub access_log: Option<String>,
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
    pub circuit_breaker: Option<CircuitBreaker>,
    pub health_check: Option<HealthCheck>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CircuitBreaker {
    pub max_connections: usize,
    pub max_pending_requests: usize,
    pub max_retries: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HealthCheck {
    pub path: String,
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
    pub timeout_ms: Option<u64>,
    pub retry: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
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
}
