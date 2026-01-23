mod common;

use common::base_config;
use pavis::regex_validator::validate_and_compile_regexes;
use pavis::router::Router;
use pavis_core::limits::RegexLimits;
use pavis_core::{
    Destination, HeaderMatch, HeaderPredicate, HeaderPredicates, Host, HttpMethod, MethodPredicate,
    Path, PathMatch, RetryPolicy, Rewrite, RewriteHost, RewritePath, RouteAction, RouteMatcher,
    Timeout, Upstream, UpstreamBuilder, UpstreamId, UpstreamName, VirtualHost, Weight,
};
use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU16;

fn upstream(name: &str, id: u16, port: u16) -> Upstream {
    UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(id).unwrap()))
        .name(UpstreamName(name.to_string()))
        .discovery(pavis_core::Discovery::Static)
        .add_endpoint(pavis_core::Endpoint {
            address: pavis_core::EndpointAddr::Ip {
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
fn test_multi_method_routing() {
    let mut config = base_config();
    config.upstreams.push(upstream("multi-backend", 1, 8081));
    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/multi".to_string()),
                },
                method: MethodPredicate::List(vec![HttpMethod::GET, HttpMethod::POST]),
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
                upstream: UpstreamName("multi-backend".to_string()),
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }]),
        }],
    });

    let router = Router::new(config.routes).expect("router");

    // GET matches
    let req = request_header("GET");
    assert!(
        router
            .match_request(None, "/multi", "GET", &req)
            .selection
            .is_some()
    );

    // POST matches
    let req = request_header("POST");
    assert!(
        router
            .match_request(None, "/multi", "POST", &req)
            .selection
            .is_some()
    );

    // PUT misses
    let req = request_header("PUT");
    assert!(
        router
            .match_request(None, "/multi", "PUT", &req)
            .selection
            .is_none()
    );
}

#[test]
fn test_header_prefix_routing() {
    let mut config = base_config();
    config.upstreams.push(upstream("prefix-backend", 1, 8082));
    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/prefix".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::Some(vec![HeaderPredicate {
                    name: "x-tenant".into(),
                    matcher: HeaderMatch::Prefix("team-".into()),
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
                upstream: UpstreamName("prefix-backend".to_string()),
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }]),
        }],
    });

    let router = Router::new(config.routes).expect("router");

    // Exact match of prefix
    let mut req = request_header("GET");
    req.insert_header("x-tenant", "team-alpha").unwrap();
    assert!(
        router
            .match_request(None, "/prefix", "GET", &req)
            .selection
            .is_some()
    );

    // Partial match of prefix
    let mut req = request_header("GET");
    req.insert_header("x-tenant", "team-beta").unwrap();
    assert!(
        router
            .match_request(None, "/prefix", "GET", &req)
            .selection
            .is_some()
    );

    // No match
    let mut req = request_header("GET");
    req.insert_header("x-tenant", "user-123").unwrap();
    assert!(
        router
            .match_request(None, "/prefix", "GET", &req)
            .selection
            .is_none()
    );
}

#[test]
fn test_header_absent_routing() {
    let mut config = base_config();
    config.upstreams.push(upstream("absent-backend", 1, 8083));
    config.routes.push(VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/absent".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::Some(vec![HeaderPredicate {
                    name: "x-internal".into(),
                    matcher: HeaderMatch::Absent,
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
                upstream: UpstreamName("absent-backend".to_string()),
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }]),
        }],
    });

    let router = Router::new(config.routes).expect("router");

    // Header absent matches
    let req = request_header("GET");
    assert!(
        router
            .match_request(None, "/absent", "GET", &req)
            .selection
            .is_some()
    );

    // Header present misses
    let mut req = request_header("GET");
    req.insert_header("x-internal", "true").unwrap();
    assert!(
        router
            .match_request(None, "/absent", "GET", &req)
            .selection
            .is_none()
    );
}

#[test]
fn test_regex_cache_integration() {
    let mut config = base_config();
    let vhost = VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/regex".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::Some(vec![HeaderPredicate {
                    name: "x-version".into(),
                    matcher: HeaderMatch::Regex("v[0-9]+".into()),
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
            action: RouteAction::Forward(vec![]),
        }],
    };
    config.routes.push(vhost);

    let mut builder = pavis_core::RuntimeConfigBuilder::new()
        .telemetry(config.telemetry)
        .shutdown(config.shutdown)
        .admin(config.admin);
    for listener in config.listeners {
        builder = builder.add_listener(listener);
    }
    for upstream in config.upstreams {
        builder = builder.add_upstream(upstream);
    }
    for route in config.routes {
        builder = builder.add_route(route);
    }
    let runtime_config = builder.build().expect("build config");

    let limits = RegexLimits::default();
    let cache = validate_and_compile_regexes(&runtime_config, &limits).expect("compile regex");

    let router = Router::with_regex(runtime_config.routes, cache, limits).expect("router");

    // Valid regex match
    let mut req = request_header("GET");
    req.insert_header("x-version", "v123").unwrap();
    assert!(
        router
            .match_request(None, "/regex", "GET", &req)
            .selection
            .is_some()
    );

    // Invalid regex match
    let mut req = request_header("GET");
    req.insert_header("x-version", "vABC").unwrap();
    assert!(
        router
            .match_request(None, "/regex", "GET", &req)
            .selection
            .is_none()
    );

    // Input too large (default limit 4096)
    let mut req = request_header("GET");
    req.insert_header("x-version", "v".to_string() + &"1".repeat(5000))
        .unwrap();
    let verdict = router.match_request(None, "/regex", "GET", &req);
    assert!(verdict.selection.is_none());
    assert_eq!(verdict.stats.regex_input_too_large, 1);
}
