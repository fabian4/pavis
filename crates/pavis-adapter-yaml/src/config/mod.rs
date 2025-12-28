//! Configuration types for Pavis proxy (Input/Adapter Layer).
//!
//! These types are used for parsing YAML/JSON configuration and validating it
//! before converting it to the efficient `pavis_core::RuntimeConfig`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Deref;

pub mod validation;

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
        // We can't use pavis_core::format_address here unless we make it public and depend on it.
        // But pavis-adapter depends on pavis-core, so it should be fine if it's public.
        // Let's assume we can use a simple format! for now or duplicate the logic if needed.
        // Or better, use pavis_core::format_address if it is available.
        // Checking pavis-core/src/lib.rs, format_address IS public.
        pavis_core::format_address(&self.ip, self.port)
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

impl TryFrom<YamlConfig> for pavis_core::RuntimeConfig {
    type Error = anyhow::Error;

    fn try_from(src: YamlConfig) -> Result<Self, Self::Error> {
        let mut upstreams = Vec::new();
        for u in src.upstreams {
            let lb = match u.load_balancer {
                LoadBalancer::Random => pavis_core::LoadBalancer::Random,
                LoadBalancer::RoundRobin => pavis_core::LoadBalancer::RoundRobin,
            };

            let mut endpoints = Vec::new();
            for e in u.endpoints {
                endpoints.push(pavis_core::Endpoint {
                    ip: e.ip,
                    port: e.port,
                    weight: e.weight.unwrap_or(1),
                });
            }

            let http_version = match u.http_version {
                HttpVersion::H1 => pavis_core::HttpVersion::H1,
                HttpVersion::H2 => pavis_core::HttpVersion::H2,
                HttpVersion::H2H1 => pavis_core::HttpVersion::H2H1,
            };

            let connection_pool = pavis_core::ConnectionPoolConfig {
                idle_timeout_secs: u.connection_pool.idle_timeout.as_secs(),
                connection_timeout_secs: u.connection_pool.connection_timeout.as_secs(),
            };

            let tls = u.tls.map(|t| pavis_core::UpstreamTlsConfig {
                enabled: t.enabled,
                verify_hostname: t.verify_hostname.unwrap_or(true),
                verify_cert: t.verify_cert.unwrap_or(true),
                sni: t.sni,
            });

            upstreams.push(pavis_core::Upstream {
                name: u.name,
                load_balancer: lb,
                http_version,
                connection_pool,
                tls,
                endpoints,
            });
        }

        let mut routes = Vec::new();
        for v in src.routes {
            let mut paths = Vec::new();
            for p in v.paths {
                let match_type = match p.match_type {
                    MatchType::Exact => pavis_core::MatchType::Exact,
                    MatchType::Regex => pavis_core::MatchType::Regex,
                    MatchType::Prefix => pavis_core::MatchType::Prefix,
                };

                let request_headers = if let Some(h) = p.request_headers {
                    let add: Vec<(String, String)> =
                        h.add.unwrap_or_default().into_iter().collect();
                    let remove = h.remove.unwrap_or_default();
                    Some(pavis_core::HeaderOperations { add, remove })
                } else {
                    None
                };

                let response_headers = if let Some(h) = p.response_headers {
                    let add: Vec<(String, String)> =
                        h.add.unwrap_or_default().into_iter().collect();
                    let remove = h.remove.unwrap_or_default();
                    Some(pavis_core::HeaderOperations { add, remove })
                } else {
                    None
                };

                let destinations = p
                    .destinations
                    .into_iter()
                    .map(|d| pavis_core::WeightedDestination {
                        upstream: d.upstream,
                        weight: d.weight,
                    })
                    .collect();

                let timeout_ms = p.timeout.map(|d| d.as_millis() as u64);
                let retry_policy = p.retry.map(|r| pavis_core::RetryPolicy {
                    attempts: r.attempts as u32,
                    per_try_timeout_ms: r.per_try_timeout.as_millis() as u64,
                    retry_on: r.retry_on.iter().map(|v| v.to_string()).collect(),
                });

                paths.push(pavis_core::Route {
                    match_type,
                    path: p.path,
                    timeout_ms,
                    retry_policy,
                    request_headers,
                    response_headers,
                    destinations,
                });
            }

            routes.push(pavis_core::VirtualHost {
                host: v.host,
                paths,
            });
        }

        Ok(pavis_core::RuntimeConfig {
            server: pavis_core::ServerConfig {
                listen_addr: src.server.listen_addr,
                worker_threads: src.server.worker_threads.map(|w| w as u64),
                tls: src.server.tls.map(|t| pavis_core::TlsConfig {
                    enabled: t.enabled,
                    cert_path: t.cert_path,
                    key_path: t.key_path,
                }),
            },
            telemetry: pavis_core::TelemetryConfig {
                level: src.telemetry.level,
                pingora: src.telemetry.pingora,
                service_name: src.telemetry.service_name,
                prometheus_addr: src.telemetry.prometheus_addr,
                access_log: match src.telemetry.access_log {
                    AccessLogConfig::False => pavis_core::AccessLogConfig::False,
                    AccessLogConfig::Stdout => pavis_core::AccessLogConfig::Stdout,
                    AccessLogConfig::File(path) => pavis_core::AccessLogConfig::File(path),
                },
                tracing: src.telemetry.tracing.map(|t| pavis_core::TracingConfig {
                    enabled: t.enabled,
                    provider: t.provider,
                    sampling_rate: t.sampling_rate,
                }),
            },
            upstreams,
            routes,
        })
    }
}

impl From<pavis_core::RuntimeConfig> for YamlConfig {
    fn from(binary: pavis_core::RuntimeConfig) -> Self {
        let mut upstreams = Vec::new();
        for u in binary.upstreams {
            let lb = match u.load_balancer {
                pavis_core::LoadBalancer::Random => LoadBalancer::Random,
                pavis_core::LoadBalancer::RoundRobin => LoadBalancer::RoundRobin,
            };

            let mut endpoints = Vec::new();
            for e in u.endpoints {
                endpoints.push(Endpoint {
                    ip: e.ip,
                    port: e.port,
                    weight: Some(e.weight),
                });
            }

            let http_version = match u.http_version {
                pavis_core::HttpVersion::H1 => HttpVersion::H1,
                pavis_core::HttpVersion::H2 => HttpVersion::H2,
                pavis_core::HttpVersion::H2H1 => HttpVersion::H2H1,
            };

            let connection_pool = ConnectionPoolConfig {
                idle_timeout: std::time::Duration::from_secs(u.connection_pool.idle_timeout_secs),
                connection_timeout: std::time::Duration::from_secs(
                    u.connection_pool.connection_timeout_secs,
                ),
            };

            let tls = u.tls.map(|t| UpstreamTlsConfig {
                enabled: t.enabled,
                verify_hostname: Some(t.verify_hostname),
                verify_cert: Some(t.verify_cert),
                sni: t.sni,
            });

            upstreams.push(Upstream {
                name: u.name,
                load_balancer: lb,
                http_version,
                connection_pool,
                tls,
                circuit_breaker: None,
                health_check: None,
                endpoints,
            });
        }

        let mut routes = Vec::new();
        for v in binary.routes {
            let mut paths = Vec::new();
            for p in v.paths {
                let match_type = match p.match_type {
                    pavis_core::MatchType::Exact => MatchType::Exact,
                    pavis_core::MatchType::Regex => MatchType::Regex,
                    pavis_core::MatchType::Prefix => MatchType::Prefix,
                };

                let request_headers = p.request_headers.map(|h| HeaderOperations {
                    add: Some(h.add.into_iter().collect()),
                    remove: Some(h.remove),
                });

                let response_headers = p.response_headers.map(|h| HeaderOperations {
                    add: Some(h.add.into_iter().collect()),
                    remove: Some(h.remove),
                });

                let destinations = p
                    .destinations
                    .into_iter()
                    .map(|d| WeightedDestination {
                        upstream: d.upstream,
                        weight: d.weight,
                    })
                    .collect();

                let timeout = p.timeout_ms.map(std::time::Duration::from_millis);
                let retry = p.retry_policy.map(|r| RetryPolicy {
                    attempts: r.attempts as usize,
                    per_try_timeout: std::time::Duration::from_millis(r.per_try_timeout_ms),
                    // This is lossy if we just to_string'd it, but for now it's fine
                    retry_on: r
                        .retry_on
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                });

                paths.push(Route {
                    match_type,
                    path: p.path,
                    timeout,
                    retry,
                    request_headers,
                    response_headers,
                    destinations,
                    compiled_regex: None,
                });
            }

            routes.push(VirtualHost {
                host: v.host,
                paths,
            });
        }

        YamlConfig {
            server: ServerConfig {
                listen_addr: binary.server.listen_addr,
                worker_threads: binary.server.worker_threads.map(|w| w as usize),
                tls: binary.server.tls.map(|t| TlsConfig {
                    enabled: t.enabled,
                    cert_path: t.cert_path,
                    key_path: t.key_path,
                }),
            },
            telemetry: TelemetryConfig {
                level: binary.telemetry.level,
                pingora: binary.telemetry.pingora,
                service_name: binary.telemetry.service_name,
                prometheus_addr: binary.telemetry.prometheus_addr,
                access_log: match binary.telemetry.access_log {
                    pavis_core::AccessLogConfig::False => AccessLogConfig::False,
                    pavis_core::AccessLogConfig::Stdout => AccessLogConfig::Stdout,
                    pavis_core::AccessLogConfig::File(path) => AccessLogConfig::File(path),
                },
                tracing: binary.telemetry.tracing.map(|t| TracingConfig {
                    enabled: t.enabled,
                    provider: t.provider,
                    sampling_rate: t.sampling_rate,
                }),
            },
            upstreams,
            routes,
        }
    }
}
