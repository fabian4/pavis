use crate::config as c;
use pavis_core::{HttpVersion, LoadBalancer, MatchType, RuntimeConfig};

/// Converts the binary protocol struct into the runtime configuration DTO.
///
/// This acts as an adapter/anti-corruption layer, ensuring that the runtime
/// works with its preferred data structure regardless of the binary format.
pub fn to_runtime_config(binary: RuntimeConfig) -> c::Config {
    let mut upstreams = Vec::new();
    for u in binary.upstreams {
        let lb = match u.load_balancer {
            LoadBalancer::Random => c::LoadBalancer::Random,
            LoadBalancer::RoundRobin => c::LoadBalancer::RoundRobin,
        };

        let mut endpoints = Vec::new();
        for e in u.endpoints {
            endpoints.push(c::Endpoint {
                ip: e.ip,
                port: e.port,
                weight: Some(e.weight),
            });
        }

        let http_version = match u.http_version {
            HttpVersion::H1 => c::HttpVersion::H1,
            HttpVersion::H2 => c::HttpVersion::H2,
            HttpVersion::H2H1 => c::HttpVersion::H2H1,
        };

        // Note: binary types are NOT automatically imported unless use'd,
        // but since they are fields of WireConfig structs, we access them via `u`.
        // However, enum variants need qualification or import.
        // Wait, WireConfig::HttpVersion is not valid syntax if HttpVersion is a sibling enum in lib.rs.
        // It is `pavis_core::HttpVersion`. I need to import it properly or use qualified path.

        let connection_pool = c::ConnectionPoolConfig {
            idle_timeout: std::time::Duration::from_secs(u.connection_pool.idle_timeout_secs),
            connection_timeout: std::time::Duration::from_secs(
                u.connection_pool.connection_timeout_secs,
            ),
        };

        let tls = u.tls.map(|t| c::UpstreamTlsConfig {
            enabled: t.enabled,
            verify_hostname: Some(t.verify_hostname),
            verify_cert: Some(t.verify_cert),
            sni: t.sni,
        });

        upstreams.push(c::Upstream {
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
                MatchType::Exact => c::MatchType::Exact,
                MatchType::Regex => c::MatchType::Regex,
                MatchType::Prefix => c::MatchType::Prefix,
            };

            let request_headers = p.request_headers.map(|h| c::HeaderOperations {
                add: Some(h.add.into_iter().collect()),
                remove: Some(h.remove),
            });

            let response_headers = p.response_headers.map(|h| c::HeaderOperations {
                add: Some(h.add.into_iter().collect()),
                remove: Some(h.remove),
            });

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| c::WeightedDestination {
                    upstream: d.upstream,
                    weight: d.weight,
                })
                .collect();

            paths.push(c::Route {
                match_type,
                path: p.path,
                timeout: None,
                retry: None,
                request_headers,
                response_headers,
                destinations,
                compiled_regex: None,
            });
        }

        routes.push(c::VirtualHost {
            host: v.host,
            paths,
        });
    }

    c::Config {
        server: c::ServerConfig {
            listen_addr: binary.listen_addr,
            worker_threads: None,
            tls: None,
        },
        telemetry: c::TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: c::AccessLogConfig::False,
            tracing: None,
        },
        upstreams,
        routes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config as c;
    use pavis_core::{Endpoint, LoadBalancer, PavisHeader, RuntimeConfig, Upstream};

    #[test]
    fn test_to_runtime_config() {
        let binary = RuntimeConfig {
            header: PavisHeader::default(),
            listen_addr: "0.0.0.0:8080".to_string(),
            upstreams: vec![Upstream {
                name: "test".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H1,
                connection_pool: pavis_core::ConnectionPoolConfig {
                    idle_timeout_secs: 60,
                    connection_timeout_secs: 5,
                },
                tls: None,
                endpoints: vec![Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 80,
                    weight: 1,
                }],
            }],
            routes: vec![],
        };

        let config = to_runtime_config(binary);

        assert_eq!(config.server.listen_addr, "0.0.0.0:8080");
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.upstreams[0].name, "test");
        assert_eq!(config.upstreams[0].endpoints[0].ip, "127.0.0.1");
        // Check enum conversion
        assert_eq!(
            config.upstreams[0].load_balancer,
            c::LoadBalancer::RoundRobin
        );
    }
}
