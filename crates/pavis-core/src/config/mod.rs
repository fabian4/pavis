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

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Deref;

mod validation;

#[derive(Debug, Clone)]
pub struct ValidatedConfig(RawConfig);

impl Deref for ValidatedConfig {
    type Target = RawConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

impl RawConfig {
    pub fn validate(self) -> Result<ValidatedConfig> {
        validation::validate(&self)?;
        Ok(ValidatedConfig(self))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MatchType {
    #[default]
    Prefix,
    Exact,
    Regex,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LoadBalancer {
    #[default]
    Random,
    RoundRobin,
}

/// HTTP version preference for upstream connections
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
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

/// Access log destination configuration
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
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
    pub unhealthy_threshold: Option<usize>,
    pub healthy_threshold: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Endpoint {
    pub ip: String,
    pub port: u16,
    pub weight: Option<u32>,
}

impl Endpoint {
    pub fn address(&self) -> String {
        crate::format_address(&self.ip, self.port)
    }
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
    // Using serde_yaml::Value or similar for mixed types might be needed if retry_on contains strings and ints,
    // but typically retry_on are status codes or strings. Let's use a custom deserializer or just Value for now
    // to match the "500, 502, 'gateway_error'" mix.
    // For simplicity here, assuming user input is consistent or we use a more flexible type.
    // The example showed [500, 502, 503, "gateway_error"...].
    // Let's use serde_json::Value to be safe.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> RawConfig {
        RawConfig {
            server: ServerConfig {
                listen_addr: "0.0.0.0:8080".to_string(),
                worker_threads: None,
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: Some("info".to_string()),
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::Stdout,
                tracing: None,
            },
            upstreams: vec![],
            routes: vec![],
        }
    }

    #[test]
    fn test_config_deserialization() {
        let mut config = base_config();
        config.telemetry.level = Some("debug".to_string());
        config.telemetry.pingora = Some("warn".to_string());

        config.upstreams.push(Upstream {
            name: "backend-v1".to_string(),
            load_balancer: LoadBalancer::Random,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![Endpoint {
                ip: "127.0.0.1".to_string(),
                port: 8081,
                weight: None,
            }],
        });

        config.upstreams.push(Upstream {
            name: "backend-v2".to_string(),
            load_balancer: LoadBalancer::Random,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![Endpoint {
                ip: "127.0.0.1".to_string(),
                port: 8082,
                weight: None,
            }],
        });

        config.routes.push(VirtualHost {
            host: "backend".to_string(),
            paths: vec![
                Route {
                    match_type: MatchType::Prefix,
                    path: "/api/v1".to_string(),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend-v1".to_string(),
                        weight: 1,
                    }],
                    compiled_regex: None,
                },
                Route {
                    match_type: MatchType::Prefix,
                    path: "/api/v2".to_string(),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend-v2".to_string(),
                        weight: 1,
                    }],
                    compiled_regex: None,
                },
            ],
        });

        assert_eq!(config.server.listen_addr, "0.0.0.0:8080");
        assert_eq!(config.telemetry.level, Some("debug".to_string()));
        assert_eq!(config.upstreams.len(), 2);
        assert_eq!(config.routes[0].paths.len(), 2);
    }

    #[test]
    fn test_http_version_variants() {
        let mut config = base_config();
        config.upstreams.push(Upstream {
            name: "h1".to_string(),
            load_balancer: LoadBalancer::Random,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![],
        });
        config.upstreams.push(Upstream {
            name: "h2".to_string(),
            load_balancer: LoadBalancer::Random,
            http_version: HttpVersion::H2,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![],
        });
        config.upstreams.push(Upstream {
            name: "h2h1".to_string(),
            load_balancer: LoadBalancer::Random,
            http_version: HttpVersion::H2H1,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![],
        });

        assert_eq!(config.upstreams[0].http_version, HttpVersion::H1);
        assert_eq!(config.upstreams[1].http_version, HttpVersion::H2);
        assert_eq!(config.upstreams[2].http_version, HttpVersion::H2H1);
    }

    #[test]
    fn test_access_log_variants() {
        let mut config = base_config();

        config.telemetry.access_log = AccessLogConfig::Stdout;
        assert_eq!(config.telemetry.access_log, AccessLogConfig::Stdout);

        config.telemetry.access_log = AccessLogConfig::False;
        assert_eq!(config.telemetry.access_log, AccessLogConfig::False);

        config.telemetry.access_log = AccessLogConfig::File("/tmp/test.log".to_string());
        assert_eq!(
            config.telemetry.access_log,
            AccessLogConfig::File("/tmp/test.log".to_string())
        );
    }

    #[test]
    fn test_connection_pool_config() {
        let mut config = base_config();
        let pool = ConnectionPoolConfig {
            idle_timeout: std::time::Duration::from_secs(120),
            connection_timeout: std::time::Duration::from_secs(10),
        };

        config.upstreams.push(Upstream {
            name: "backend".to_string(),
            load_balancer: LoadBalancer::Random,
            http_version: HttpVersion::H1,
            connection_pool: pool,
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![],
        });

        assert_eq!(
            config.upstreams[0].connection_pool.idle_timeout,
            std::time::Duration::from_secs(120)
        );
        assert_eq!(
            config.upstreams[0].connection_pool.connection_timeout,
            std::time::Duration::from_secs(10)
        );
    }

    #[test]
    fn test_tls_config() {
        let mut config = base_config();
        config.server.tls = Some(TlsConfig {
            enabled: true,
            cert_path: Some("/path/to/cert.pem".to_string()),
            key_path: Some("/path/to/key.pem".to_string()),
        });

        let tls = config.server.tls.as_ref().unwrap();
        assert!(tls.enabled);
        assert_eq!(tls.cert_path, Some("/path/to/cert.pem".to_string()));
    }

    #[test]
    fn test_empty_config_validation() {
        let config = base_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = RawConfig {
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
                    timeout: None,
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
        assert!(config.clone().validate().is_err());

        // Test invalid upstream hostname
        let mut config_invalid_host = config.clone();
        config_invalid_host.server.listen_addr = "0.0.0.0:8080".to_string();
        config_invalid_host.upstreams.push(Upstream {
            name: "bad-host".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![Endpoint {
                ip: "invalid@host".to_string(), // Invalid char '@'
                port: 80,
                weight: Some(1),
            }],
        });
        assert!(config_invalid_host.validate().is_err());

        // Test duplicate upstream names
        let mut config_duplicate = config.clone();
        config_duplicate.server.listen_addr = "0.0.0.0:8080".to_string();
        config_duplicate.upstreams.push(Upstream {
            name: "non-existent".to_string(), // Already exists in config
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![Endpoint {
                ip: "127.0.0.1".to_string(),
                port: 81,
                weight: Some(1),
            }],
        });
        assert!(config_duplicate.validate().is_err());
    }

    #[test]
    fn test_config_header_validation() {
        let mut config = RawConfig {
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
                    timeout: None,
                    retry: None,
                    request_headers: Some(HeaderOperations {
                        add: Some(HashMap::from([
                            ("Valid-Header".to_string(), "valid value".to_string()),
                            ("Invalid-Header\r\n".to_string(), "value".to_string()),
                        ])),
                        remove: None,
                    }),
                    response_headers: None,
                    destinations: vec![],
                    compiled_regex: None,
                }],
            }],
        };

        // Should fail due to invalid header name
        assert!(config.clone().validate().is_err());

        // Fix header name, break header value
        config.routes[0].paths[0].request_headers = Some(HeaderOperations {
            add: Some(HashMap::from([(
                "Valid-Header".to_string(),
                "valid value\r\nInjected".to_string(),
            )])),
            remove: None,
        });
        assert!(config.clone().validate().is_err());

        // Valid headers
        config.routes[0].paths[0].request_headers = Some(HeaderOperations {
            add: Some(HashMap::from([(
                "Valid-Header".to_string(),
                "valid value".to_string(),
            )])),
            remove: None,
        });
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_regex_validation() {
        let mut config = base_config();
        config.routes.push(VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Regex,
                path: "/api/v1/(".to_string(), // Invalid regex (unclosed parenthesis)
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![],
                compiled_regex: None,
            }],
        });

        // Should fail due to invalid regex
        assert!(config.clone().validate().is_err());

        // Fix regex
        config.routes[0].paths[0].path = "/api/v1/(.*)".to_string();
        assert!(config.validate().is_ok());
    }
}
