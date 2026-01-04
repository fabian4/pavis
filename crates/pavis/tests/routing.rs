mod common;

use common::base_config;
use pavis::router::Router;
use pavis_core::{
    ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType, Route, Upstream,
    VirtualHost, WeightedDestination,
};
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn test_routing_prefix_match() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "backend-a".to_string(),
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
                8081,
            )),
            weight: 1,
        }],
    });
    config.routes.push(VirtualHost {
        host: "*".to_string(),
        paths: vec![Route {
            match_type: MatchType::Prefix,
            path: "/api".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            rewrite: None,
            destinations: vec![WeightedDestination {
                upstream: "backend-a".to_string(),
                weight: 1,
            }],
        }],
    });

    let router = Router::new(config.routes).expect("Failed to create router");

    // Match /api
    let (_vhost, route) = router
        .match_request(None, "/api/users")
        .expect("Should match");
    assert_eq!(route.destinations[0].upstream, "backend-a");

    // No match
    assert!(router.match_request(None, "/other").is_none());
}

#[test]
fn test_routing_exact_and_regex_match() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "backend-exact".to_string(),
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
                8082,
            )),
            weight: 1,
        }],
    });
    config.upstreams.push(Upstream {
        name: "backend-regex".to_string(),
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
                8083,
            )),
            weight: 1,
        }],
    });
    config.routes.push(VirtualHost {
        host: "*".to_string(),
        paths: vec![
            Route {
                match_type: MatchType::Exact,
                path: "/health".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: "backend-exact".to_string(),
                    weight: 1,
                }],
            },
            Route {
                match_type: MatchType::Regex,
                path: r"^/items/[0-9]+$".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: "backend-regex".to_string(),
                    weight: 1,
                }],
            },
        ],
    });

    let router = Router::new(config.routes).expect("Failed to create router");

    let (_vhost, route) = router
        .match_request(None, "/health")
        .expect("Should match exact");
    assert_eq!(route.destinations[0].upstream, "backend-exact");

    let (_vhost, route) = router
        .match_request(None, "/items/42")
        .expect("Should match regex");
    assert_eq!(route.destinations[0].upstream, "backend-regex");

    assert!(router.match_request(None, "/items/abc").is_none());
}
