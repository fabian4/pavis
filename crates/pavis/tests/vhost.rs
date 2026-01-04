mod common;

use common::base_config;
use pavis::router::Router;
use pavis_core::{
    ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType, Route, Upstream,
    VirtualHost, WeightedDestination,
};
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn test_routing_vhost_precedence() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "api-upstream".to_string(),
        discovery_type: pavis_core::DiscoveryType::Static,
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![Endpoint {
            address: pavis_core::EndpointAddress::Ip(std::net::SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                8084,
            )),
            weight: 1,
        }],
    });
    config.upstreams.push(Upstream {
        name: "web-upstream".to_string(),
        discovery_type: pavis_core::DiscoveryType::Static,
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![Endpoint {
            address: pavis_core::EndpointAddress::Ip(std::net::SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                8085,
            )),
            weight: 1,
        }],
    });
    config.upstreams.push(Upstream {
        name: "wildcard-upstream".to_string(),
        discovery_type: pavis_core::DiscoveryType::Static,
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![Endpoint {
            address: pavis_core::EndpointAddress::Ip(std::net::SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                8086,
            )),
            weight: 1,
        }],
    });
    config.routes = vec![
        VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Exact,
                path: "/".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: "wildcard-upstream".to_string(),
                    weight: 1,
                }],
            }],
        },
        VirtualHost {
            host: "api.com".to_string(),
            paths: vec![Route {
                match_type: MatchType::Exact,
                path: "/".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: "api-upstream".to_string(),
                    weight: 1,
                }],
            }],
        },
        VirtualHost {
            host: "web.com".to_string(),
            paths: vec![Route {
                match_type: MatchType::Exact,
                path: "/".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: "web-upstream".to_string(),
                    weight: 1,
                }],
            }],
        },
    ];

    let router = Router::new(config.routes).expect("Failed to create router");

    let (vhost, _route) = router
        .match_request(Some("api.com"), "/")
        .expect("api.com should match");
    assert_eq!(vhost.host, "api.com");

    let (vhost, _route) = router
        .match_request(Some("web.com"), "/")
        .expect("web.com should match");
    assert_eq!(vhost.host, "web.com");

    let (vhost, _route) = router
        .match_request(Some("unknown.com"), "/")
        .expect("wildcard should match");
    assert_eq!(vhost.host, "*");
}
