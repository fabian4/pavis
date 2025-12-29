use pavis_core::{self as binary};
use std::fmt::Write;

pub fn format_header(header: &pavis_pvs::PvsHeader) -> String {
    let mut out = String::new();
    let magic = std::str::from_utf8(&header.magic).unwrap_or("????");
    writeln!(&mut out, "--- Pavis Header ---").ok();
    writeln!(&mut out, "Magic: {magic:?}").ok();
    writeln!(&mut out, "Version: {}", header.version).ok();
    writeln!(&mut out, "Algorithm: {}", header.algorithm).ok();
    writeln!(&mut out, "Checksum: {}", hex::encode(header.checksum)).ok();
    writeln!(&mut out).ok();
    out
}

pub fn format_config(config: &binary::RuntimeConfig) -> String {
    let mut out = String::new();
    writeln!(
        &mut out,
        "--- Config Tree ---\nListen Address: {}",
        config.server.listen_addr
    )
    .ok();
    writeln!(&mut out, "Upstreams ({}):", config.upstreams.len()).ok();
    for upstream in &config.upstreams {
        let lb_str = match upstream.load_balancer {
            binary::LoadBalancer::RoundRobin => "RoundRobin",
            binary::LoadBalancer::Random => "Random",
        };
        let hv_str = match upstream.http_version {
            binary::HttpVersion::H1 => "H1",
            binary::HttpVersion::H2 => "H2",
            binary::HttpVersion::H2H1 => "H2H1",
        };
        writeln!(
            &mut out,
            "- Upstream: {}, LB: {}, HTTP: {}, endpoints: {}",
            upstream.name,
            lb_str,
            hv_str,
            upstream.endpoints.len()
        )
        .ok();
        for endpoint in &upstream.endpoints {
            writeln!(
                &mut out,
                "  - {}:{} weight={}",
                endpoint.ip, endpoint.port, endpoint.weight
            )
            .ok();
        }
    }

    writeln!(&mut out, "Routes ({}):", config.routes.len()).ok();
    for vhost in &config.routes {
        writeln!(&mut out, "Host: {}", vhost.host).ok();
        for route in &vhost.paths {
            let match_type = match route.match_type {
                binary::MatchType::Prefix => "prefix",
                binary::MatchType::Exact => "exact",
                binary::MatchType::Regex => "regex",
            };
            writeln!(&mut out, "  - [{match_type}] {}", route.path).ok();
            for dest in &route.destinations {
                writeln!(
                    &mut out,
                    "      -> {} (weight {})",
                    dest.upstream, dest.weight
                )
                .ok();
            }
        }
    }

    out
}

pub fn format_stats(config: &binary::RuntimeConfig, total_bytes: u64) -> String {
    let mut out = String::new();
    let header_size = pavis_pvs::HEADER_SIZE as u64;
    let payload_size = total_bytes.saturating_sub(header_size);
    let endpoints: usize = config.upstreams.iter().map(|u| u.endpoints.len()).sum();
    let routes: usize = config.routes.iter().map(|v| v.paths.len()).sum();
    let destinations: usize = config
        .routes
        .iter()
        .flat_map(|v| &v.paths)
        .map(|r| r.destinations.len())
        .sum();

    writeln!(&mut out, "--- Binary Stats ---").ok();
    writeln!(&mut out, "Total Size: {total_bytes} bytes").ok();
    writeln!(&mut out, "Header Size: {header_size} bytes").ok();
    writeln!(&mut out, "Payload Size: {payload_size} bytes").ok();
    writeln!(&mut out).ok();

    writeln!(&mut out, "--- Structure Stats ---").ok();
    writeln!(&mut out, "Upstreams: {}", config.upstreams.len()).ok();
    writeln!(&mut out, "Endpoints: {endpoints}").ok();
    writeln!(&mut out, "Virtual Hosts: {}", config.routes.len()).ok();
    writeln!(&mut out, "Routes: {routes}").ok();
    writeln!(&mut out, "Destinations: {destinations}").ok();
    writeln!(&mut out).ok();
    out
}

#[cfg(test)]
mod tests {
    use super::{format_config, format_header, format_stats};
    use pavis_core::{
        AccessLogConfig, ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType,
        Route, RuntimeConfig, ServerConfig, TelemetryConfig, Upstream, VirtualHost,
        WeightedDestination,
    };
    use pavis_pvs::{PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn format_header_emits_expected_fields() {
        let header = PvsHeader {
            magic: *PAVIS_MAGIC,
            version: PAVIS_VERSION,
            algorithm: PAVIS_HASH_ALGORITHM_SHA256,
            checksum: [0xAB; 32],
            _reserved: [0; 20],
        };
        let output = format_header(&header);
        assert!(output.contains("--- Pavis Header ---"));
        assert!(output.contains("Magic: \"PAVS\""));
        assert!(output.contains("Version: 0"));
        assert!(output.contains("Algorithm: 1"));
        assert!(output.contains(&hex::encode([0xAB; 32])));
    }

    #[test]
    fn format_config_emits_routes_and_upstreams() {
        let config = RuntimeConfig {
            server: ServerConfig {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                worker_threads: None,
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::Disabled,
                tracing: None,
            },
            upstreams: vec![Upstream {
                name: "backend".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H2,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 60,
                    connection_timeout_secs: 5,
                },
                tls: None,
                endpoints: vec![Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                    port: 8081,
                    weight: 1,
                }],
            }],
            routes: vec![VirtualHost {
                host: "example.com".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Exact,
                    path: "/health".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend".to_string(),
                        weight: 1,
                    }],
                    compiled_regex: None,
                }],
            }],
        };

        let output = format_config(&config);
        assert!(output.contains("Listen Address: 127.0.0.1:8080"));
        assert!(output.contains("- Upstream: backend, LB: RoundRobin, HTTP: H2"));
        assert!(output.contains("Host: example.com"));
        assert!(output.contains("[exact] /health"));
        assert!(output.contains("-> backend (weight 1)"));
    }

    #[test]
    fn format_stats_emits_sizes_and_counts() {
        let config = RuntimeConfig {
            server: ServerConfig {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                worker_threads: None,
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::Disabled,
                tracing: None,
            },
            upstreams: vec![Upstream {
                name: "backend".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H2,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 60,
                    connection_timeout_secs: 5,
                },
                tls: None,
                endpoints: vec![Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                    port: 8081,
                    weight: 1,
                }],
            }],
            routes: vec![VirtualHost {
                host: "example.com".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Exact,
                    path: "/health".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend".to_string(),
                        weight: 1,
                    }],
                    compiled_regex: None,
                }],
            }],
        };

        let output = format_stats(&config, 1024);
        assert!(output.contains("--- Binary Stats ---"));
        assert!(output.contains("Total Size: 1024 bytes"));
        assert!(output.contains("Header Size: 64 bytes"));
        assert!(output.contains("--- Structure Stats ---"));
        assert!(output.contains("Upstreams: 1"));
        assert!(output.contains("Endpoints: 1"));
        assert!(output.contains("Virtual Hosts: 1"));
        assert!(output.contains("Routes: 1"));
        assert!(output.contains("Destinations: 1"));
    }
}
