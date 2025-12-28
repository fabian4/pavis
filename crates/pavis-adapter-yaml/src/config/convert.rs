use anyhow::{Context, Result};
use std::net::{IpAddr, SocketAddr};

use pavis_core::{self, LogLevel, validate_runtime};

use super::types::*;

impl TryFrom<YamlConfig> for pavis_core::RuntimeConfig {
    type Error = anyhow::Error;

    fn try_from(src: YamlConfig) -> Result<Self, Self::Error> {
        let listen_addr: SocketAddr = src
            .server
            .listen_addr
            .parse()
            .context("Invalid listen_addr")?;

        let mut upstreams = Vec::new();
        for u in src.upstreams {
            let mut endpoints = Vec::new();
            for e in u.endpoints {
                let ip: IpAddr = e.ip.parse().with_context(|| {
                    format!("Invalid endpoint IP '{}' for upstream '{}'", e.ip, u.name)
                })?;
                endpoints.push(pavis_core::Endpoint {
                    ip,
                    port: e.port,
                    weight: e.weight.unwrap_or(1),
                });
            }

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
                load_balancer: u.load_balancer,
                http_version: u.http_version,
                connection_pool,
                tls,
                endpoints,
            });
        }

        let mut routes = Vec::new();
        for v in src.routes {
            let mut paths = Vec::new();
            for p in v.paths {
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
                let retry_policy = if let Some(r) = p.retry {
                    let retry_on = r
                        .retry_on
                        .iter()
                        .map(|v| {
                            v.as_str().map(str::to_string).ok_or_else(|| {
                                anyhow::anyhow!("retry.retry_on entries must be strings")
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    Some(pavis_core::RetryPolicy {
                        attempts: r.attempts as u32,
                        per_try_timeout_ms: r.per_try_timeout.as_millis() as u64,
                        retry_on,
                    })
                } else {
                    None
                };

                paths.push(pavis_core::Route {
                    match_type: p.match_type,
                    path: p.path,
                    timeout_ms,
                    retry_policy,
                    request_headers,
                    response_headers,
                    destinations,
                    compiled_regex: None,
                });
            }

            routes.push(pavis_core::VirtualHost {
                host: v.host,
                paths,
            });
        }

        let runtime = pavis_core::RuntimeConfig {
            server: pavis_core::ServerConfig {
                listen_addr,
                worker_threads: src.server.worker_threads.map(|w| w as u64),
                tls: src.server.tls.map(|t| pavis_core::TlsConfig {
                    enabled: t.enabled,
                    cert_path: t.cert_path,
                    key_path: t.key_path,
                }),
            },
            telemetry: pavis_core::TelemetryConfig {
                level: parse_log_level(src.telemetry.level),
                pingora: parse_log_level(src.telemetry.pingora),
                service_name: src.telemetry.service_name,
                prometheus_addr: src.telemetry.prometheus_addr,
                access_log: src.telemetry.access_log,
                tracing: src.telemetry.tracing.map(|t| pavis_core::TracingConfig {
                    enabled: t.enabled,
                    provider: t.provider,
                    sampling_rate: t.sampling_rate,
                }),
            },
            upstreams,
            routes,
        };

        validate_runtime(&runtime).map_err(anyhow::Error::from)?;
        Ok(runtime)
    }
}

fn parse_log_level(level: Option<String>) -> Option<LogLevel> {
    level.and_then(|l| match l.to_lowercase().as_str() {
        "error" => Some(LogLevel::Error),
        "warn" => Some(LogLevel::Warn),
        "info" => Some(LogLevel::Info),
        "debug" => Some(LogLevel::Debug),
        "trace" => Some(LogLevel::Trace),
        _ => None, // Fallback to None (or could error, but Option is safe)
    })
}

fn log_level_to_string(level: Option<LogLevel>) -> Option<String> {
    level.map(|l| match l {
        LogLevel::Error => "error".to_string(),
        LogLevel::Warn => "warn".to_string(),
        LogLevel::Info => "info".to_string(),
        LogLevel::Debug => "debug".to_string(),
        LogLevel::Trace => "trace".to_string(),
    })
}

impl From<pavis_core::RuntimeConfig> for YamlConfig {
    fn from(binary: pavis_core::RuntimeConfig) -> Self {
        let mut upstreams = Vec::new();
        for u in binary.upstreams {
            let mut endpoints = Vec::new();
            for e in u.endpoints {
                endpoints.push(Endpoint {
                    ip: e.ip.to_string(),
                    port: e.port,
                    weight: Some(e.weight),
                });
            }

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
                load_balancer: u.load_balancer,
                http_version: u.http_version,
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
                    retry_on: r
                        .retry_on
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                });

                paths.push(Route {
                    match_type: p.match_type,
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
                listen_addr: binary.server.listen_addr.to_string(),
                worker_threads: binary.server.worker_threads.map(|w| w as usize),
                tls: binary.server.tls.map(|t| TlsConfig {
                    enabled: t.enabled,
                    cert_path: t.cert_path,
                    key_path: t.key_path,
                }),
            },
            telemetry: TelemetryConfig {
                level: log_level_to_string(binary.telemetry.level),
                pingora: log_level_to_string(binary.telemetry.pingora),
                service_name: binary.telemetry.service_name,
                prometheus_addr: binary.telemetry.prometheus_addr,
                access_log: binary.telemetry.access_log,
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

#[cfg(test)]
mod tests {
    use crate::config::types::YamlConfig;
    use pavis_core::{
        AccessLogConfig, ConnectionPoolConfig, Endpoint, HeaderOperations, HttpVersion,
        LoadBalancer, LogLevel, MatchType, RetryPolicy, Route, RuntimeConfig, ServerConfig,
        TelemetryConfig, Upstream, UpstreamTlsConfig, VirtualHost, WeightedDestination,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;

    #[test]
    fn yaml_to_runtime_converts_defaults_and_structures() {
        let yaml = r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    tls:
      enabled: true
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        timeout: "1s"
        request_headers:
          add:
            x-added: "1"
        response_headers:
          remove: ["x-remove"]
        retry:
          attempts: 2
          per_try_timeout: "250ms"
          retry_on: ["5xx", "connect-failure"]
        destinations:
          - upstream: "backend"
            weight: 1
"#;

        let config = YamlConfig::parse_str(yaml).expect("parse yaml");
        let runtime: RuntimeConfig = config.try_into().expect("convert to runtime");

        let upstream = &runtime.upstreams[0];
        assert_eq!(upstream.endpoints[0].weight, 1);
        assert_eq!(upstream.connection_pool.idle_timeout_secs, 60);
        assert_eq!(upstream.connection_pool.connection_timeout_secs, 5);
        let tls = upstream.tls.as_ref().expect("tls config");
        assert!(tls.verify_hostname);
        assert!(tls.verify_cert);

        let route = &runtime.routes[0].paths[0];
        assert_eq!(route.timeout_ms, Some(1000));
        let retry = route.retry_policy.as_ref().expect("retry policy");
        assert_eq!(retry.attempts, 2);
        assert_eq!(retry.per_try_timeout_ms, 250);
        assert_eq!(
            retry.retry_on,
            vec!["5xx".to_string(), "connect-failure".to_string()]
        );
        let request_headers = route.request_headers.as_ref().expect("request headers");
        assert_eq!(
            request_headers.add,
            vec![("x-added".to_string(), "1".to_string())]
        );
        assert!(request_headers.remove.is_empty());
        let response_headers = route.response_headers.as_ref().expect("response headers");
        assert!(response_headers.add.is_empty());
        assert_eq!(response_headers.remove, vec!["x-remove".to_string()]);
    }

    #[test]
    fn runtime_to_yaml_preserves_values() {
        let runtime = RuntimeConfig {
            server: ServerConfig {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                worker_threads: Some(2),
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: Some(LogLevel::Info),
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::False,
                tracing: None,
            },
            upstreams: vec![Upstream {
                name: "backend".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H2,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 10,
                    connection_timeout_secs: 2,
                },
                tls: Some(UpstreamTlsConfig {
                    enabled: false,
                    verify_hostname: false,
                    verify_cert: false,
                    sni: Some("backend.local".to_string()),
                }),
                endpoints: vec![Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: 8081,
                    weight: 3,
                }],
            }],
            routes: vec![VirtualHost {
                host: "example.com".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Exact,
                    path: "/".to_string(),
                    timeout_ms: Some(1500),
                    retry_policy: Some(RetryPolicy {
                        attempts: 3,
                        per_try_timeout_ms: 500,
                        retry_on: vec!["5xx".to_string()],
                    }),
                    request_headers: Some(HeaderOperations {
                        add: vec![("x-add".to_string(), "1".to_string())],
                        remove: vec!["x-remove".to_string()],
                    }),
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend".to_string(),
                        weight: 2,
                    }],
                    compiled_regex: None,
                }],
            }],
        };

        let config: YamlConfig = runtime.into();
        assert_eq!(config.server.listen_addr, "127.0.0.1:8080");
        assert_eq!(config.server.worker_threads, Some(2));
        assert_eq!(config.telemetry.level, Some("info".to_string()));
        assert_eq!(config.telemetry.access_log, AccessLogConfig::False);
        let upstream = &config.upstreams[0];
        assert_eq!(upstream.load_balancer, LoadBalancer::RoundRobin);
        assert_eq!(upstream.http_version, HttpVersion::H2);
        assert_eq!(
            upstream.connection_pool.idle_timeout,
            Duration::from_secs(10)
        );
        assert_eq!(
            upstream.connection_pool.connection_timeout,
            Duration::from_secs(2)
        );
        let tls = upstream.tls.as_ref().expect("tls config");
        assert_eq!(tls.enabled, false);
        assert_eq!(tls.verify_hostname, Some(false));
        assert_eq!(tls.verify_cert, Some(false));
        assert_eq!(tls.sni.as_deref(), Some("backend.local"));
        assert_eq!(upstream.endpoints[0].weight, Some(3));
        assert_eq!(upstream.endpoints[0].ip, "127.0.0.1");

        let route = &config.routes[0].paths[0];
        assert_eq!(route.match_type, MatchType::Exact);
        assert_eq!(route.timeout, Some(Duration::from_millis(1500)));
        let retry = route.retry.as_ref().expect("retry policy");
        assert_eq!(retry.attempts, 3);
        assert_eq!(retry.per_try_timeout, Duration::from_millis(500));
        assert_eq!(
            retry.retry_on,
            vec![serde_json::Value::String("5xx".to_string())]
        );
        let request_headers = route.request_headers.as_ref().expect("request headers");
        assert_eq!(
            request_headers.add.as_ref().expect("add headers")["x-add"],
            "1"
        );
        assert_eq!(
            request_headers.remove.as_ref().expect("remove headers"),
            &vec!["x-remove".to_string()]
        );
    }
}
