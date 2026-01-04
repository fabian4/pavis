use pavis_core::{
    ConnectionPoolConfig, DiscoveryType, Endpoint, EndpointAddress, HttpVersion, Listener,
    LoadBalancer, MatchType, Route, RuntimeConfig, TelemetryConfig, Upstream, VirtualHost,
    WeightedDestination,
};
use std::net::SocketAddr;

pub fn runtime_config(
    listen_addr: SocketAddr,
    upstream_a: (&str, SocketAddr),
    upstream_b: (&str, SocketAddr),
    route_upstream: &str,
) -> RuntimeConfig {
    RuntimeConfig {
        listeners: vec![Listener {
            name: "default".to_string(),
            listen_addr,
            worker_threads: None,
            tls: None,
        }],
        telemetry: TelemetryConfig {
            level: None,
            pingora: None,
            service_name: Some("pavis-integrated".to_string()),
            prometheus_addr: None,
            access_log: Default::default(),
            tracing: None,
        },
        upstreams: vec![
            upstream(upstream_a.0, upstream_a.1),
            upstream(upstream_b.0, upstream_b.1),
        ],
        routes: vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Prefix,
                path: "/".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: route_upstream.to_string(),
                    weight: 1,
                }],
            }],
        }],
    }
}

pub fn upstream(name: &str, addr: SocketAddr) -> Upstream {
    Upstream {
        name: name.to_string(),
        discovery_type: DiscoveryType::Static,
        load_balancer: LoadBalancer::RoundRobin,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![Endpoint {
            address: EndpointAddress::Ip(addr),
            weight: 1,
        }],
    }
}
