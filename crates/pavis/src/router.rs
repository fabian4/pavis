//! Router module: Request matching and routing logic.
//!
//! # Architectural Invariants
//!
//! 1. **Deterministic Matching**: Route matching order is significant and must be deterministic.
//! 2. **Pre-compiled Regex**: All regular expressions must be compiled at initialization time, never during request handling.
//! 3. **Read-Only**: The router state is immutable after initialization.

use anyhow::{Context, Result};
use pavis_core::{HeaderPredicates, MethodPredicate, PathMatch, VirtualHost};
use regex::Regex;
use std::collections::HashMap;

pub mod matcher;
pub use matcher::{MatchVerdict, PredicateStats};

#[derive(Clone, Debug)]
pub(crate) struct CompiledRoute {
    pub index: usize,
    pub regex: Option<Regex>,
}

#[derive(Debug)]
pub(crate) enum RouteZone {
    Linear(Vec<CompiledRoute>),
    ExactMap(HashMap<String, CompiledRoute>),
}

pub(crate) struct CompiledVirtualHost {
    pub config: VirtualHost,
    pub zones: Vec<RouteZone>,
}

pub struct Router {
    pub(crate) exact_hosts: HashMap<String, CompiledVirtualHost>,
    pub(crate) wildcard_hosts: Vec<CompiledVirtualHost>,
}

impl Router {
    pub fn new(routes: Vec<VirtualHost>) -> Result<Self> {
        let mut exact_hosts = HashMap::new();
        let mut wildcard_hosts = Vec::new();

        for vhost in routes {
            let mut zones = Vec::new();
            let mut current_linear: Option<Vec<CompiledRoute>> = None;
            let mut current_map: Option<HashMap<String, CompiledRoute>> = None;

            for (index, route) in vhost.paths.iter().enumerate() {
                // Routes with method or header predicates MUST use Linear zone for sequential matching
                let has_predicates = !matches!(route.matcher.method, MethodPredicate::Any)
                    || !matches!(route.matcher.headers, HeaderPredicates::None);

                match &route.matcher.path {
                    PathMatch::Exact { path } if !has_predicates => {
                        // Flush linear if exists
                        if let Some(linear) = current_linear.take() {
                            zones.push(RouteZone::Linear(linear));
                        }
                        // Add to map (only for routes without method/header predicates)
                        let compiled = CompiledRoute { index, regex: None };
                        if let Some(map) = &mut current_map {
                            map.entry(path.0.clone()).or_insert(compiled);
                        } else {
                            let mut map = HashMap::new();
                            map.insert(path.0.clone(), compiled);
                            current_map = Some(map);
                        }
                    }
                    PathMatch::Exact { .. }
                    | PathMatch::Prefix { .. }
                    | PathMatch::Regex { .. } => {
                        // Flush map if exists
                        if let Some(map) = current_map.take() {
                            zones.push(RouteZone::ExactMap(map));
                        }
                        // Add to linear (includes Exact routes with predicates)
                        let regex = match &route.matcher.path {
                            PathMatch::Regex { path } => {
                                Some(Regex::new(&path.0).with_context(|| {
                                    format!("Failed to compile regex for path: {}", path.0)
                                })?)
                            }
                            _ => None,
                        };
                        let compiled = CompiledRoute { index, regex };
                        if let Some(linear) = &mut current_linear {
                            linear.push(compiled);
                        } else {
                            current_linear = Some(vec![compiled]);
                        }
                    }
                    #[allow(unreachable_patterns)]
                    &_ => {
                        // Unknown matcher type - skip it
                        continue;
                    }
                }
            }

            // Flush remaining
            if let Some(linear) = current_linear {
                zones.push(RouteZone::Linear(linear));
            }
            if let Some(map) = current_map {
                zones.push(RouteZone::ExactMap(map));
            }

            let compiled_vhost = CompiledVirtualHost {
                config: vhost,
                zones,
            };

            if compiled_vhost.config.host.0 == "*" || compiled_vhost.config.host.0.contains('*') {
                wildcard_hosts.push(compiled_vhost);
            } else {
                exact_hosts.insert(compiled_vhost.config.host.0.clone(), compiled_vhost);
            }
        }
        Ok(Self {
            exact_hosts,
            wildcard_hosts,
        })
    }

    pub fn match_request<'a>(
        &'a self,
        host_header: Option<&str>,
        uri_path: &str,
        method: &str,
        headers: &pingora::http::RequestHeader,
    ) -> MatchVerdict<'a> {
        matcher::match_request(self, host_header, uri_path, method, headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        Destination, HeaderPredicates, HeadersPolicy, Host, MethodPredicate, Path, PathMatch,
        RetryPolicy, Rewrite, RewriteHost, RewritePath, Route, RouteAction, RouteMatcher, Timeout,
        UpstreamName, Weight,
    };
    use std::num::NonZeroU16;

    fn request_header(method: &str) -> pingora::http::RequestHeader {
        pingora::http::RequestHeader::build(method, b"/", None).expect("request header")
    }

    fn create_routes() -> Vec<VirtualHost> {
        vec![
            VirtualHost {
                host: Host("example.com".to_string()),
                paths: vec![
                    Route {
                        matcher: RouteMatcher {
                            path: PathMatch::Exact {
                                path: Path("/exact".to_string()),
                            },
                            method: MethodPredicate::Any,
                            headers: HeaderPredicates::None,
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: HeadersPolicy::Disabled.into(),
                        response_headers: HeadersPolicy::Disabled.into(),
                        principal: pavis_core::Principal::Any,
                        rewrite: Rewrite {
                            path: RewritePath::Disabled,
                            host: RewriteHost::Disabled,
                        },
                        action: RouteAction::Forward(vec![Destination {
                            upstream: UpstreamName("backend-1".to_string()),
                            weight: Weight(NonZeroU16::new(1).unwrap()),
                        }]),
                    },
                    Route {
                        matcher: RouteMatcher {
                            path: PathMatch::Prefix {
                                path: Path("/api".to_string()),
                            },
                            method: MethodPredicate::Any,
                            headers: HeaderPredicates::None,
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: HeadersPolicy::Disabled.into(),
                        response_headers: HeadersPolicy::Disabled.into(),
                        principal: pavis_core::Principal::Any,
                        rewrite: Rewrite {
                            path: RewritePath::Disabled,
                            host: RewriteHost::Disabled,
                        },
                        action: RouteAction::Forward(vec![Destination {
                            upstream: UpstreamName("backend-1".to_string()),
                            weight: Weight(NonZeroU16::new(1).unwrap()),
                        }]),
                    },
                ],
            },
            VirtualHost {
                host: Host("*".to_string()),
                paths: vec![Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Prefix {
                            path: Path("/public".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled.into(),
                    response_headers: HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("backend-2".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }]),
                }],
            },
        ]
    }

    #[test]
    fn test_invalid_regex_compilation() {
        let routes = vec![VirtualHost {
            host: Host("*".to_string()),
            paths: vec![Route {
                matcher: RouteMatcher {
                    path: PathMatch::Regex {
                        path: Path("[unclosed".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled.into(),
                response_headers: HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![]),
            }],
        }];

        assert!(Router::new(routes).is_err());
    }

    #[test]
    fn test_find_route_exact_match() {
        let router = Router::new(create_routes()).unwrap();
        let req = request_header("GET");
        let (vhost, route) = router
            .match_request(Some("example.com"), "/exact", "GET", &req)
            .into_option()
            .expect("Should match");
        assert_eq!(vhost.host.0, "example.com");
        match &route.matcher.path {
            PathMatch::Exact { path } => assert_eq!(path.0, "/exact"),
            _ => panic!("expected exact match"),
        }
    }

    #[test]
    fn test_find_route_prefix_match() {
        let router = Router::new(create_routes()).unwrap();
        let req = request_header("GET");
        let (vhost, route) = router
            .match_request(Some("example.com"), "/api/v1/users", "GET", &req)
            .into_option()
            .expect("Should match");
        assert_eq!(vhost.host.0, "example.com");
        match &route.matcher.path {
            PathMatch::Prefix { path } => assert_eq!(path.0, "/api"),
            _ => panic!("expected prefix match"),
        }
    }

    #[test]
    fn test_find_route_wildcard_host() {
        let router = Router::new(create_routes()).unwrap();
        let req = request_header("GET");
        let (vhost, route) = router
            .match_request(Some("any.com"), "/public/stuff", "GET", &req)
            .into_option()
            .expect("Should match");
        assert_eq!(vhost.host.0, "*");
        match &route.matcher.path {
            PathMatch::Prefix { path } => assert_eq!(path.0, "/public"),
            _ => panic!("expected prefix match"),
        }
    }

    #[test]
    fn test_find_route_no_match() {
        let router = Router::new(create_routes()).unwrap();
        let req = request_header("GET");
        let result = router.match_request(Some("example.com"), "/notfound", "GET", &req);
        assert!(result.into_option().is_none());
    }

    #[test]
    fn test_find_route_wrong_host() {
        let router = Router::new(create_routes()).unwrap();
        let req = request_header("GET");
        let result = router.match_request(Some("other.com"), "/exact", "GET", &req);
        assert!(result.into_option().is_none());
    }

    #[test]
    fn test_find_route_regex_match() {
        let routes = vec![VirtualHost {
            host: Host("*".to_string()),
            paths: vec![Route {
                matcher: RouteMatcher {
                    path: PathMatch::Regex {
                        path: Path(r"^/api/v[0-9]+/users/\d+$".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled.into(),
                response_headers: HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: RouteAction::Forward(vec![Destination {
                    upstream: UpstreamName("backend".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            }],
        }];

        let router = Router::new(routes).unwrap();
        let req = request_header("GET");

        let result = router.match_request(None, "/api/v1/users/123", "GET", &req);
        assert!(result.selection.is_some());
        let (_, route) = result.into_option().unwrap();
        matches!(route.matcher.path, PathMatch::Regex { .. });

        let result = router.match_request(None, "/api/v2/users/456", "GET", &req);
        assert!(result.selection.is_some());

        let result = router.match_request(None, "/api/v1/users/", "GET", &req);
        assert!(result.into_option().is_none());

        let result = router.match_request(None, "/api/v1/users/abc", "GET", &req);
        assert!(result.into_option().is_none());
    }

    #[test]
    fn test_route_order_precedence() {
        let routes = vec![VirtualHost {
            host: Host("*".to_string()),
            paths: vec![
                Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Prefix {
                            path: Path("/app".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled.into(),
                    response_headers: HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("backend-1".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }]),
                },
                Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Exact {
                            path: Path("/app".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled.into(),
                    response_headers: HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("backend-2".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }]),
                },
            ],
        }];

        let router = Router::new(routes).unwrap();
        let req = request_header("GET");
        let (_, route) = router
            .match_request(None, "/app", "GET", &req)
            .into_option()
            .expect("match");
        matches!(route.matcher.path, PathMatch::Prefix { .. });
    }

    #[test]
    fn test_exact_map_optimization_preserves_order() {
        let routes = vec![VirtualHost {
            host: Host("*".to_string()),
            paths: vec![
                // Zone 1: Linear (Prefix)
                Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Prefix {
                            path: Path("/a".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled.into(),
                    response_headers: HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![]),
                },
                // Zone 2: ExactMap (consecutive exacts)
                Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Exact {
                            path: Path("/b".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled.into(),
                    response_headers: HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("b1".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }]),
                },
                Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Exact {
                            path: Path("/b".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled.into(),
                    response_headers: HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("b2".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }]),
                },
            ],
        }];

        let router = Router::new(routes).unwrap();
        let req = request_header("GET");
        // Check zones structure implicitly by matching
        // /b should match the FIRST exact match in the map block
        let (_, route) = router
            .match_request(None, "/b", "GET", &req)
            .into_option()
            .expect("match");
        match &route.action {
            RouteAction::Forward(destinations) => {
                assert_eq!(destinations[0].upstream.0, "b1");
            }
            _ => panic!("expected Forward action"),
        }

        // /a should match the prefix
        let (_, route) = router
            .match_request(None, "/a/foo", "GET", &req)
            .into_option()
            .expect("match");
        match &route.matcher.path {
            PathMatch::Prefix { path } => assert_eq!(path.0, "/a"),
            _ => panic!("expected prefix"),
        }
    }
}
