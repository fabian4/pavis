use anyhow::Result;

use pavis_core::{self, validate_runtime_config};

use super::types::*;

impl TryFrom<YamlConfig> for pavis_core::RuntimeConfig {
    type Error = anyhow::Error;

    fn try_from(src: YamlConfig) -> Result<Self, Self::Error> {
        let mut upstreams = Vec::new();
        for u in src.upstreams {
            let mut endpoints = Vec::new();
            for e in u.endpoints {
                endpoints.push(pavis_core::Endpoint {
                    ip: e.ip,
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
                let retry_policy = p.retry.map(|r| pavis_core::RetryPolicy {
                    attempts: r.attempts as u32,
                    per_try_timeout_ms: r.per_try_timeout.as_millis() as u64,
                    retry_on: r.retry_on.iter().map(|v| v.to_string()).collect(),
                });

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

        validate_runtime_config(&runtime).map_err(anyhow::Error::from)?;
        Ok(runtime)
    }
}

impl From<pavis_core::RuntimeConfig> for YamlConfig {
    fn from(binary: pavis_core::RuntimeConfig) -> Self {
        let mut upstreams = Vec::new();
        for u in binary.upstreams {
            let mut endpoints = Vec::new();
            for e in u.endpoints {
                endpoints.push(Endpoint {
                    ip: e.ip,
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
