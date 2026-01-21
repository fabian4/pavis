mod common;

use common::base_config;
use pavis::router::Router;
use pavis_core::{
    ConnectTimeout, ConnectionLimit, Destination, Duration, Endpoint, EndpointAddr, HeaderMatch,
    HeaderPredicate, HeaderPredicates, Host, HttpMethod, HttpVersion, IdleTimeout, LoadBalancer,
    MethodPredicate, Path, PathMatch, Pool, RetryPolicy, Rewrite, RewriteHost, RewritePath,
    RouteAction, RouteMatcher, Timeout, Upstream, UpstreamBuilder, UpstreamId, UpstreamName,
    VirtualHost, Weight,
};
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU16, NonZeroU32};

fn upstream(name: &str, id: u16, port: u16) -> Upstream {
    UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(id).unwrap()))
        .name(UpstreamName(name.to_string()))
        .discovery(pavis_core::Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(pavis_core::TlsPolicy::Disabled)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: pavis_core::Port(NonZeroU16::new(port).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream")
}

fn request_header(method: &str) -> pingora::http::RequestHeader {
    pingora::http::RequestHeader::build(method, b"/", None).expect("request header")
}

#[test]
fn test_routing_prefix_match() {
    let mut config = base_config();
    config.upstreams.push(upstream("backend-a", 1, 8081));
    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/api".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: pavis_core::HeadersPolicy::Disabled.into(),
            response_headers: pavis_core::HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
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
    let req = request_header("GET");

    let (_vhost, route) = router
        .match_request(None, "/api/users", "GET", &req)
        .into_option()
        .expect("Should match");
    match &route.action {
        RouteAction::Forward(destinations) => {
            assert_eq!(destinations[0].upstream.0, "backend-a");
        }
        _ => panic!("expected Forward action"),
    }

    assert!(
        router
            .match_request(None, "/other", "GET", &req)
            .into_option()
            .is_none()
    );
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
                matcher: RouteMatcher {
                    path: PathMatch::Exact {
                        path: Path("/health".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
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
                matcher: RouteMatcher {
                    path: PathMatch::Regex {
                        path: Path(r"^/items/[0-9]+$".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
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
    let req = request_header("GET");

    let (_vhost, route) = router
        .match_request(None, "/health", "GET", &req)
        .into_option()
        .expect("Should match exact");
    match &route.action {
        RouteAction::Forward(destinations) => {
            assert_eq!(destinations[0].upstream.0, "backend-exact");
        }
        _ => panic!("expected Forward action"),
    }

    let (_vhost, route) = router
        .match_request(None, "/items/42", "GET", &req)
        .into_option()
        .expect("Should match regex");
    match &route.action {
        RouteAction::Forward(destinations) => {
            assert_eq!(destinations[0].upstream.0, "backend-regex");
        }
        _ => panic!("expected Forward action"),
    }

    assert!(
        router
            .match_request(None, "/items/abc", "GET", &req)
            .into_option()
            .is_none()
    );
}

#[test]
fn test_routing_method_predicate_selection() {
    let mut config = base_config();
    config.upstreams.push(upstream("get-backend", 1, 8087));
    config.upstreams.push(upstream("post-backend", 2, 8088));
    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![
            pavis_core::Route {
                matcher: RouteMatcher {
                    path: PathMatch::Exact {
                        path: Path("/resource".to_string()),
                    },
                    method: MethodPredicate::Specific(HttpMethod::GET),
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("get-backend".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
            pavis_core::Route {
                matcher: RouteMatcher {
                    path: PathMatch::Exact {
                        path: Path("/resource".to_string()),
                    },
                    method: MethodPredicate::Specific(HttpMethod::POST),
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("post-backend".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
        ],
    });

    let router = Router::new(config.routes).expect("router");
    let get_req = request_header("GET");
    let post_req = request_header("POST");
    let put_req = request_header("PUT");

    let (_, route) = router
        .match_request(None, "/resource", "GET", &get_req)
        .into_option()
        .expect("GET route");
    if let RouteAction::Forward(destinations) = &route.action {
        assert_eq!(destinations[0].upstream.0, "get-backend");
    } else {
        panic!("expected forward");
    }

    let (_, route) = router
        .match_request(None, "/resource", "POST", &post_req)
        .into_option()
        .expect("POST route");
    if let RouteAction::Forward(destinations) = &route.action {
        assert_eq!(destinations[0].upstream.0, "post-backend");
    } else {
        panic!("expected forward");
    }

    assert!(
        router
            .match_request(None, "/resource", "PUT", &put_req)
            .into_option()
            .is_none()
    );
}

#[test]
fn test_routing_header_predicates() {
    let mut config = base_config();
    config.upstreams.push(upstream("tenant-alpha", 1, 8090));
    config.upstreams.push(upstream("tenant-default", 2, 8091));
    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![
            pavis_core::Route {
                matcher: RouteMatcher {
                    path: PathMatch::Exact {
                        path: Path("/tenant".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::Some(vec![HeaderPredicate {
                        name: "x-tenant".into(),
                        matcher: HeaderMatch::Exact("alpha".into()),
                    }]),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("tenant-alpha".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
            pavis_core::Route {
                matcher: RouteMatcher {
                    path: PathMatch::Exact {
                        path: Path("/tenant".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("tenant-default".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
        ],
    });

    let router = Router::new(config.routes).expect("router");
    let mut alpha_req = request_header("GET");
    alpha_req
        .insert_header("x-tenant", "alpha")
        .expect("header");
    let default_req = request_header("GET");

    let (_, route) = router
        .match_request(None, "/tenant", "GET", &alpha_req)
        .into_option()
        .expect("alpha tenant route");
    if let RouteAction::Forward(destinations) = &route.action {
        assert_eq!(destinations[0].upstream.0, "tenant-alpha");
    } else {
        panic!("expected forward");
    }

    let (_, route) = router
        .match_request(None, "/tenant", "GET", &default_req)
        .into_option()
        .expect("default tenant route");
    if let RouteAction::Forward(destinations) = &route.action {
        assert_eq!(destinations[0].upstream.0, "tenant-default");
    } else {
        panic!("expected forward");
    }
}

#[test]
fn test_routing_tie_breaking() {
    let mut config = base_config();
    config.upstreams.push(upstream("backend-first", 1, 8081));
    config.upstreams.push(upstream("backend-second", 2, 8082));

    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![
            // First route with /api prefix
            pavis_core::Route {
                matcher: RouteMatcher {
                    path: PathMatch::Prefix {
                        path: Path("/api".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("backend-first".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
            // Second route with identical /api prefix (should never match due to tie-breaking)
            pavis_core::Route {
                matcher: RouteMatcher {
                    path: PathMatch::Prefix {
                        path: Path("/api".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("backend-second".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            },
        ],
    });

    let router = Router::new(config.routes).expect("router");
    let req = request_header("GET");

    // All requests to /api should match the FIRST route (tie-breaking)
    let (_, route) = router
        .match_request(None, "/api/users", "GET", &req)
        .into_option()
        .expect("should match first route");
    if let RouteAction::Forward(destinations) = &route.action {
        assert_eq!(
            destinations[0].upstream.0, "backend-first",
            "tie-breaking: first route in config order should win"
        );
    } else {
        panic!("expected forward");
    }

    // Verify with different path under /api
    let (_, route) = router
        .match_request(None, "/api/v2/products", "GET", &req)
        .into_option()
        .expect("should match first route");
    if let RouteAction::Forward(destinations) = &route.action {
        assert_eq!(
            destinations[0].upstream.0, "backend-first",
            "tie-breaking: first route should consistently win"
        );
    } else {
        panic!("expected forward");
    }
}
