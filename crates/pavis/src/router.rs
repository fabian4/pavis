//! Router module: Request matching and routing logic.
//!
//! # Architectural Invariants
//!
//! 1. **Deterministic Matching**: Route matching order is significant and must be deterministic.
//! 2. **Pre-compiled Regex**: All regular expressions must be compiled at initialization time, never during request handling.
//! 3. **Read-Only**: The router state is immutable after initialization.

use anyhow::{Context, Result};
use pavis_core::{PathMatch, Route, VirtualHost};
use regex::Regex;
use std::collections::HashMap;

pub mod matcher;

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
    pub(crate) wildcard_exact: HashMap<String, Vec<usize>>,
    pub(crate) wildcard_all: Vec<usize>,
}

impl Router {
    pub fn new(routes: Vec<VirtualHost>) -> Result<Self> {
        let mut exact_hosts = HashMap::new();
        let mut wildcard_hosts = Vec::new();
        let mut wildcard_exact: HashMap<String, Vec<usize>> = HashMap::new();
        let mut wildcard_all = Vec::new();

        for vhost in routes {
            let mut zones = Vec::new();
            let mut current_linear: Option<Vec<CompiledRoute>> = None;
            let mut current_map: Option<HashMap<String, CompiledRoute>> = None;

            for (index, route) in vhost.paths.iter().enumerate() {
                match &route.matcher {
                    PathMatch::Exact { path } => {
                        // Flush linear if exists
                        if let Some(linear) = current_linear.take() {
                            zones.push(RouteZone::Linear(linear));
                        }
                        // Add to map
                        let compiled = CompiledRoute { index, regex: None };
                        if let Some(map) = &mut current_map {
                            // If key exists, we must NOT overwrite it because the FIRST match wins.
                            // However, if we are in the SAME map zone, it means there are no intervening
                            // regex/prefix routes.
                            // So if we see duplicate Exact paths in sequence:
                            // 1. Exact /a -> A
                            // 2. Exact /a -> B
                            // The map will store A (first insert wins).
                            // This preserves the "First Match" semantics for Exact matches in a block.
                            map.entry(path.0.clone()).or_insert(compiled);
                        } else {
                            let mut map = HashMap::new();
                            map.insert(path.0.clone(), compiled);
                            current_map = Some(map);
                        }
                    }
                    PathMatch::Prefix { .. } | PathMatch::Regex { .. } => {
                        // Flush map if exists
                        if let Some(map) = current_map.take() {
                            zones.push(RouteZone::ExactMap(map));
                        }
                        // Add to linear
                        let regex = match &route.matcher {
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
                let index = wildcard_hosts.len();
                let host = compiled_vhost.config.host.0.clone();
                wildcard_hosts.push(compiled_vhost);
                if host == "*" {
                    wildcard_all.push(index);
                } else {
                    wildcard_exact.entry(host).or_default().push(index);
                }
            } else {
                exact_hosts.insert(compiled_vhost.config.host.0.clone(), compiled_vhost);
            }
        }
        Ok(Self {
            exact_hosts,
            wildcard_hosts,
            wildcard_exact,
            wildcard_all,
        })
    }

    pub fn match_request<'a>(
        &'a self,
        host_header: Option<&str>,
        uri_path: &str,
    ) -> Option<(&'a VirtualHost, &'a Route)> {
        matcher::match_request(self, host_header, uri_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        Destination, HeadersPolicy, Host, Path, PathMatch, RetryPolicy, Rewrite, RewriteHost,
        RewritePath, RouteAction, Timeout, UpstreamName, Weight,
    };
    use std::num::NonZeroU16;

    fn create_routes() -> Vec<VirtualHost> {
        vec![
            VirtualHost {
                host: Host("example.com".to_string()),
                paths: vec![
                    Route {
                        matcher: PathMatch::Exact {
                            path: Path("/exact".to_string()),
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: HeadersPolicy::Disabled,
                        response_headers: HeadersPolicy::Disabled,
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
                        matcher: PathMatch::Prefix {
                            path: Path("/api".to_string()),
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: HeadersPolicy::Disabled,
                        response_headers: HeadersPolicy::Disabled,
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
                    matcher: PathMatch::Prefix {
                        path: Path("/public".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled,
                    response_headers: HeadersPolicy::Disabled,
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
                matcher: PathMatch::Regex {
                    path: Path("[unclosed".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled,
                response_headers: HeadersPolicy::Disabled,
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
        let (vhost, route) = router
            .match_request(Some("example.com"), "/exact")
            .expect("Should match");
        assert_eq!(vhost.host.0, "example.com");
        match &route.matcher {
            PathMatch::Exact { path } => assert_eq!(path.0, "/exact"),
            _ => panic!("expected exact match"),
        }
    }

    #[test]
    fn test_find_route_prefix_match() {
        let router = Router::new(create_routes()).unwrap();
        let (vhost, route) = router
            .match_request(Some("example.com"), "/api/v1/users")
            .expect("Should match");
        assert_eq!(vhost.host.0, "example.com");
        match &route.matcher {
            PathMatch::Prefix { path } => assert_eq!(path.0, "/api"),
            _ => panic!("expected prefix match"),
        }
    }

    #[test]
    fn test_find_route_wildcard_host() {
        let router = Router::new(create_routes()).unwrap();
        let (vhost, route) = router
            .match_request(Some("any.com"), "/public/stuff")
            .expect("Should match");
        assert_eq!(vhost.host.0, "*");
        match &route.matcher {
            PathMatch::Prefix { path } => assert_eq!(path.0, "/public"),
            _ => panic!("expected prefix match"),
        }
    }

    #[test]
    fn test_find_route_no_match() {
        let router = Router::new(create_routes()).unwrap();
        let result = router.match_request(Some("example.com"), "/notfound");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_route_wrong_host() {
        let router = Router::new(create_routes()).unwrap();
        let result = router.match_request(Some("other.com"), "/exact");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_route_regex_match() {
        let routes = vec![VirtualHost {
            host: Host("*".to_string()),
            paths: vec![Route {
                matcher: PathMatch::Regex {
                    path: Path(r"^/api/v[0-9]+/users/\d+$".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled,
                response_headers: HeadersPolicy::Disabled,
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

        let result = router.match_request(None, "/api/v1/users/123");
        assert!(result.is_some());
        let (_, route) = result.unwrap();
        matches!(route.matcher, PathMatch::Regex { .. });

        let result = router.match_request(None, "/api/v2/users/456");
        assert!(result.is_some());

        let result = router.match_request(None, "/api/v1/users/");
        assert!(result.is_none());

        let result = router.match_request(None, "/api/v1/users/abc");
        assert!(result.is_none());
    }

    #[test]
    fn test_route_order_precedence() {
        let routes = vec![VirtualHost {
            host: Host("*".to_string()),
            paths: vec![
                Route {
                    matcher: PathMatch::Prefix {
                        path: Path("/app".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled,
                    response_headers: HeadersPolicy::Disabled,
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
                    matcher: PathMatch::Exact {
                        path: Path("/app".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled,
                    response_headers: HeadersPolicy::Disabled,
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
        let (_, route) = router.match_request(None, "/app").expect("match");
        matches!(route.matcher, PathMatch::Prefix { .. });
    }

    #[test]
    fn test_exact_map_optimization_preserves_order() {
        let routes = vec![VirtualHost {
            host: Host("*".to_string()),
            paths: vec![
                // Zone 1: Linear (Prefix)
                Route {
                    matcher: PathMatch::Prefix {
                        path: Path("/a".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled,
                    response_headers: HeadersPolicy::Disabled,
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![]),
                },
                // Zone 2: ExactMap (consecutive exacts)
                Route {
                    matcher: PathMatch::Exact {
                        path: Path("/b".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled,
                    response_headers: HeadersPolicy::Disabled,
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
                    matcher: PathMatch::Exact {
                        path: Path("/b".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled,
                    response_headers: HeadersPolicy::Disabled,
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
        // Check zones structure implicitly by matching
        // /b should match the FIRST exact match in the map block
        let (_, route) = router.match_request(None, "/b").expect("match");
        match &route.action {
            RouteAction::Forward(destinations) => {
                assert_eq!(destinations[0].upstream.0, "b1");
            }
            _ => panic!("expected Forward action"),
        }

        // /a should match the prefix
        let (_, route) = router.match_request(None, "/a/foo").expect("match");
        match &route.matcher {
            PathMatch::Prefix { path } => assert_eq!(path.0, "/a"),
            _ => panic!("expected prefix"),
        }
    }
}
