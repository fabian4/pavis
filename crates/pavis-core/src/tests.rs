use crate::runtime::{
    AccessLogConfig, ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, LogLevel, MatchType,
    RetryPolicy, Route, RuntimeConfig, ServerConfig, TelemetryConfig, TlsConfig, TracingConfig,
    Upstream, UpstreamTlsConfig, VirtualHost, WeightedDestination,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
fn test_config_structure() {
    let config = RuntimeConfig {
        server: ServerConfig {
            listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080),
            worker_threads: Some(4),
            tls: None,
        },
        telemetry: TelemetryConfig {
            level: Some(LogLevel::Info),
            pingora: None,
            service_name: Some("test".to_string()),
            prometheus_addr: Some("0.0.0.0:9090".to_string()),
            access_log: AccessLogConfig::Stdout,
            tracing: None,
        },
        upstreams: vec![Upstream {
            name: "upstream1".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: 8080,
                weight: 1,
            }],
        }],
        routes: vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Prefix,
                path: "/".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![WeightedDestination {
                    upstream: "upstream1".to_string(),
                    weight: 1,
                }],
                compiled_regex: None,
            }],
        }],
    };

    assert_eq!(config.server.worker_threads, Some(4));
    assert_eq!(config.upstreams.len(), 1);
    assert_eq!(config.routes.len(), 1);
}