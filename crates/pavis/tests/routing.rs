mod common;

use common::base_config;
use pavis::router::Router;
use pavis_core::{
    ConnectTimeout, ConnectionLimit, Destination, Duration, Endpoint, EndpointAddr, Host,
    HttpVersion, IdleTimeout, LoadBalancer, Path, PathMatch, Pool, RetryPolicy, Rewrite,
    RewriteHost, RewritePath, RouteAction, Timeout, Upstream, UpstreamId, UpstreamName,
    VirtualHost, Weight,
};
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU16, NonZeroU32};

fn upstream(name: &str, id: u16, port: u16) -> Upstream {
    Upstream {
        id: UpstreamId(NonZeroU16::new(id).unwrap()),
        name: UpstreamName(name.to_string()),
        discovery: pavis_core::Discovery::Static,
        balancer: LoadBalancer::Random,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit::Unlimited,
        },
        tls: pavis_core::TlsPolicy::Disabled,
        endpoints: vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: pavis_core::Port(NonZeroU16::new(port).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    }
}

#[test]
fn test_routing_prefix_match() {
    let mut config = base_config();
    config.upstreams.push(upstream("backend-a", 1, 8081));
    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Prefix {
                path: Path("/api".to_string()),
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: pavis_core::HeadersPolicy::Disabled,
            response_headers: pavis_core::HeadersPolicy::Disabled,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![Destination {
                upstream: UpstreamName("backend-a".to_string()),
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }]),
        }],
    });

    let router = Router::new(config.routes).expect("Failed to create router");

    let (_vhost, route) = router
        .match_request(None, "/api/users")
        .expect("Should match");
    match &route.action {
        RouteAction::Forward(destinations) => {
            assert_eq!(destinations[0].upstream.0, "backend-a");
        }
        _ => panic!("expected Forward action"),
    }

    assert!(router.match_request(None, "/other").is_none());
}

#[test]
fn test_routing_exact_and_regex_match() {
    let mut config = base_config();
    config.upstreams.push(upstream("backend-exact", 1, 8082));
    config.upstreams.push(upstream("backend-regex", 2, 8083));
    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![
            pavis_core::Route {
                matcher: PathMatch::Exact {
                    path: Path("/health".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled,
                response_headers: pavis_core::HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("backend-exact".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
            pavis_core::Route {
                matcher: PathMatch::Regex {
                    path: Path(r"^/items/[0-9]+$".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled,
                response_headers: pavis_core::HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("backend-regex".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
        ],
    });

    let router = Router::new(config.routes).expect("Failed to create router");

    let (_vhost, route) = router
        .match_request(None, "/health")
        .expect("Should match exact");
    match &route.action {
        RouteAction::Forward(destinations) => {
            assert_eq!(destinations[0].upstream.0, "backend-exact");
        }
        _ => panic!("expected Forward action"),
    }

    let (_vhost, route) = router
        .match_request(None, "/items/42")
        .expect("Should match regex");
    match &route.action {
        RouteAction::Forward(destinations) => {
            assert_eq!(destinations[0].upstream.0, "backend-regex");
        }
        _ => panic!("expected Forward action"),
    }

    assert!(router.match_request(None, "/items/abc").is_none());
}
