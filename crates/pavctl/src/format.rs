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
    writeln!(&mut out, "--- Config Tree ---").ok();
    writeln!(&mut out, "Listeners ({})", config.listeners.len()).ok();
    for listener in &config.listeners {
        writeln!(&mut out, "- Name: {}", listener.name.0).ok();
        writeln!(&mut out, "  Address: {}", listener.address).ok();
    }

    writeln!(&mut out, "Upstreams ({})", config.upstreams.len()).ok();
    for upstream in &config.upstreams {
        let lb_str = match upstream.balancer {
            binary::LoadBalancer::RoundRobin => "RoundRobin",
            binary::LoadBalancer::Random => "Random",
            binary::LoadBalancer::LeastRequest => "LeastRequest",
            _ => "Unknown",
        };
        let hv_str = match upstream.protocol {
            binary::HttpVersion::H1 => "H1",
            binary::HttpVersion::H2 => "H2",
            binary::HttpVersion::H2H1 => "H2H1",
            _ => "Unknown",
        };
        writeln!(
            &mut out,
            "- Upstream: {}, LB: {}, HTTP: {}, endpoints: {}",
            upstream.name.0,
            lb_str,
            hv_str,
            upstream.endpoints.len()
        )
        .ok();
        for endpoint in &upstream.endpoints {
            let addr_str = match &endpoint.address {
                binary::EndpointAddr::Ip { address, port } => {
                    format!("{}:{}", address, port.0)
                }
                binary::EndpointAddr::Dns { host, port } => format!("{}:{}", host.0, port.0),
                _ => "Unknown".to_string(),
            };
            writeln!(&mut out, "  - {} weight={}", addr_str, endpoint.weight.0).ok();
        }
    }

    writeln!(&mut out, "Routes ({})", config.routes.len()).ok();
    for vhost in &config.routes {
        writeln!(&mut out, "Host: {}", vhost.host.0).ok();
        for route in &vhost.paths {
            let (match_type, path) = match &route.matcher.path {
                binary::PathMatch::Prefix { path } => ("prefix", path.0.as_str()),
                binary::PathMatch::Exact { path } => ("exact", path.0.as_str()),
                binary::PathMatch::Regex { path } => ("regex", path.0.as_str()),
                _ => ("unknown", "??"),
            };
            writeln!(&mut out, "  - [{match_type}] {}", path).ok();
            match &route.action {
                binary::RouteAction::Forward(destinations) => {
                    for dest in destinations {
                        writeln!(
                            &mut out,
                            "      -> {} (weight {})",
                            dest.upstream.0, dest.weight.0
                        )
                        .ok();
                    }
                }
                binary::RouteAction::Redirect { status, location } => {
                    writeln!(&mut out, "      -> Redirect {} to {}", status, location).ok();
                }
                binary::RouteAction::Direct { status, body: _ } => {
                    writeln!(&mut out, "      -> Direct {}", status).ok();
                }
                _ => {
                    writeln!(&mut out, "      -> Unknown Action").ok();
                }
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
        .map(|r| match &r.action {
            binary::RouteAction::Forward(destinations) => destinations.len(),
            _ => 0,
        })
        .sum();

    writeln!(&mut out, "--- Binary Stats ---").ok();
    writeln!(&mut out, "Total Size: {total_bytes} bytes").ok();
    writeln!(&mut out, "Header Size: {header_size} bytes").ok();
    writeln!(&mut out, "Payload Size: {payload_size} bytes").ok();
    writeln!(&mut out).ok();

    writeln!(&mut out, "--- Structure Stats ---").ok();
    writeln!(&mut out, "Listeners: {}", config.listeners.len()).ok();
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
        AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Duration, Endpoint,
        EndpointAddr, HeaderPredicates, Host, HttpVersion, IdleTimeout, ListenerBuilder,
        ListenerName, LoadBalancer, MethodPredicate, Metrics, Path, PathMatch, Pool, Port,
        RetryPolicy, Rewrite, RewriteHost, RewritePath, RouteAction, RouteMatcher, RuntimeConfig,
        RuntimeConfigBuilder, ServiceName, Telemetry, Timeout, TlsConfig, TlsPolicy,
        UpstreamBuilder, UpstreamId, UpstreamName, VirtualHost, Weight, WorkerCount,
    };
    use pavis_pvs::{PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::{NonZeroU16, NonZeroU32};
    use std::sync::Arc;

    fn sample_config() -> RuntimeConfig {
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                8080,
            ))
            .workers(WorkerCount::Auto)
            .tls(TlsConfig::Disabled)
            .build()
            .expect("listener");

        let upstream_one = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("backend".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H2)
            .pool(Pool {
                idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
                connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8081).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .expect("upstream one");

        let upstream_two = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(2).unwrap()))
            .name(UpstreamName("backend-h2h1".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H2H1)
            .pool(Pool {
                idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(30_000).unwrap())),
                connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(3_000).unwrap())),
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
                    port: Port(NonZeroU16::new(8082).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .expect("upstream two");

        RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: ServiceName("pavis".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .add_upstream(upstream_one)
            .add_upstream(upstream_two)
            .add_route(VirtualHost {
                host: Host("example.com".to_string()),
                paths: vec![
                    pavis_core::Route {
                        matcher: RouteMatcher {
                            path: PathMatch::Exact {
                                path: Path("/health".to_string()),
                            },
                            method: MethodPredicate::Any,
                            headers: HeaderPredicates::None,
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: Arc::new(pavis_core::HeadersPolicy::Disabled),
                        response_headers: Arc::new(pavis_core::HeadersPolicy::Disabled),
                        principal: pavis_core::Principal::Any,
                        rewrite: Rewrite {
                            path: RewritePath::Disabled,
                            host: RewriteHost::Disabled,
                        },
                        action: RouteAction::Forward(vec![Destination {
                            upstream: UpstreamName("backend".to_string()),
                            weight: Weight(NonZeroU16::new(1).unwrap()),
                        }]),
                    },
                    pavis_core::Route {
                        matcher: RouteMatcher {
                            path: PathMatch::Regex {
                                path: Path("^/items/[0-9]+$".to_string()),
                            },
                            method: MethodPredicate::Any,
                            headers: HeaderPredicates::None,
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: Arc::new(pavis_core::HeadersPolicy::Disabled),
                        response_headers: Arc::new(pavis_core::HeadersPolicy::Disabled),
                        principal: pavis_core::Principal::Any,
                        rewrite: Rewrite {
                            path: RewritePath::Disabled,
                            host: RewriteHost::Disabled,
                        },
                        action: RouteAction::Forward(vec![Destination {
                            upstream: UpstreamName("backend-h2h1".to_string()),
                            weight: Weight(NonZeroU16::new(1).unwrap()),
                        }]),
                    },
                ],
            })
            .build()
            .expect("config")
    }

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
        let config = sample_config();

        let output = format_config(&config);
        assert!(output.contains("Listeners (1)"));
        assert!(output.contains("- Name: default"));
        assert!(output.contains("Address: 127.0.0.1:8080"));
        assert!(output.contains("- Upstream: backend, LB: RoundRobin, HTTP: H2"));
        assert!(output.contains("- Upstream: backend-h2h1, LB: Random, HTTP: H2H1"));
        assert!(output.contains("Host: example.com"));
        assert!(output.contains("[exact] /health"));
        assert!(output.contains("[regex] ^/items/[0-9]+$"));
        assert!(output.contains("-> backend (weight 1)"));
    }

    #[test]
    fn format_stats_emits_sizes_and_counts() {
        let config = sample_config();
        let output = format_stats(&config, 128);
        assert!(output.contains("Total Size: 128 bytes"));
        assert!(output.contains("Listeners: 1"));
        assert!(output.contains("Upstreams: 2"));
        assert!(output.contains("Routes: 2"));
        assert!(output.contains("Destinations: 2"));
    }

    #[test]
    fn format_config_variants() {
        use pavis_core::Hostname;
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
            .workers(WorkerCount::Auto)
            .tls(TlsConfig::Disabled)
            .build()
            .expect("listener");

        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("u1".to_string()))
            .discovery(pavis_core::Discovery::Logical)
            .balancer(LoadBalancer::LeastRequest)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(Endpoint {
                address: EndpointAddr::Dns {
                    host: Hostname("example.com".to_string()),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .expect("upstream");

        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: ServiceName("pavis".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .add_upstream(upstream)
            .add_route(VirtualHost {
                host: Host("vhost".to_string()),
                paths: vec![
                    pavis_core::Route {
                        matcher: RouteMatcher {
                            path: PathMatch::Prefix {
                                path: Path("/redirect".to_string()),
                            },
                            method: MethodPredicate::Any,
                            headers: HeaderPredicates::None,
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: Arc::new(pavis_core::HeadersPolicy::Disabled),
                        response_headers: Arc::new(pavis_core::HeadersPolicy::Disabled),
                        principal: pavis_core::Principal::Any,
                        rewrite: Rewrite {
                            path: RewritePath::Disabled,
                            host: RewriteHost::Disabled,
                        },
                        action: RouteAction::Redirect {
                            status: 302,
                            location: "/login".to_string(),
                        },
                    },
                    pavis_core::Route {
                        matcher: RouteMatcher {
                            path: PathMatch::Prefix {
                                path: Path("/direct".to_string()),
                            },
                            method: MethodPredicate::Any,
                            headers: HeaderPredicates::None,
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: Arc::new(pavis_core::HeadersPolicy::Disabled),
                        response_headers: Arc::new(pavis_core::HeadersPolicy::Disabled),
                        principal: pavis_core::Principal::Any,
                        rewrite: Rewrite {
                            path: RewritePath::Disabled,
                            host: RewriteHost::Disabled,
                        },
                        action: RouteAction::Direct {
                            status: 200,
                            body: "ok".to_string(),
                        },
                    },
                ],
            })
            .build()
            .expect("config");

        let output = format_config(&config);
        assert!(output.contains("LeastRequest"));
        assert!(output.contains("HTTP: H1"));
        assert!(output.contains("example.com:80"));
        assert!(output.contains("Redirect 302 to /login"));
        assert!(output.contains("Direct 200"));
    }
}
