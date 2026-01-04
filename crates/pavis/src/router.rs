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

pub mod matcher;

pub(crate) struct CompiledRoute {
    pub index: usize,
    pub regex: Option<Regex>,
}

pub(crate) struct CompiledVirtualHost {
    pub config: VirtualHost,
    pub routes: Vec<CompiledRoute>,
}

pub struct Router {
    routes: Vec<CompiledVirtualHost>,
}

impl Router {
    pub fn new(routes: Vec<VirtualHost>) -> Result<Self> {
        let mut compiled_routes = Vec::new();
        for vhost in routes {
            let mut compiled = Vec::new();
            for (index, route) in vhost.paths.iter().enumerate() {
                let regex = match &route.matcher {
                    PathMatch::Regex { path } => Some(Regex::new(&path.0).with_context(|| {
                        format!("Failed to compile regex for path: {}", path.0)
                    })?),
                    _ => None,
                };
                compiled.push(CompiledRoute { index, regex });
            }
            compiled_routes.push(CompiledVirtualHost {
                config: vhost,
                routes: compiled,
            });
        }
        Ok(Self {
            routes: compiled_routes,
        })
    }

    pub fn match_request<'a>(
        &'a self,
        host_header: Option<&str>,
        uri_path: &str,
    ) -> Option<(&'a VirtualHost, &'a Route)> {
        matcher::match_request(&self.routes, host_header, uri_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        Destination, HeadersPolicy, Host, Path, PathMatch, RetryPolicy, Rewrite, RewriteHost,
        RewritePath, Timeout, UpstreamName, Weight,
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
                        rewrite: Rewrite {
                            path: RewritePath::Disabled,
                            host: RewriteHost::Disabled,
                        },
                        destinations: vec![Destination {
                            upstream: UpstreamName("backend-1".to_string()),
                            weight: Weight(NonZeroU16::new(1).unwrap()),
                        }],
                    },
                    Route {
                        matcher: PathMatch::Prefix {
                            path: Path("/api".to_string()),
                        },
                        timeout: Timeout::Disabled,
                        retry: RetryPolicy::Disabled,
                        request_headers: HeadersPolicy::Disabled,
                        response_headers: HeadersPolicy::Disabled,
                        rewrite: Rewrite {
                            path: RewritePath::Disabled,
                            host: RewriteHost::Disabled,
                        },
                        destinations: vec![Destination {
                            upstream: UpstreamName("backend-1".to_string()),
                            weight: Weight(NonZeroU16::new(1).unwrap()),
                        }],
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
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    destinations: vec![Destination {
                        upstream: UpstreamName("backend-2".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }],
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
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                destinations: vec![],
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
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                destinations: vec![Destination {
                    upstream: UpstreamName("backend".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }],
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
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    destinations: vec![Destination {
                        upstream: UpstreamName("backend-1".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }],
                },
                Route {
                    matcher: PathMatch::Exact {
                        path: Path("/app".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled,
                    response_headers: HeadersPolicy::Disabled,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    destinations: vec![Destination {
                        upstream: UpstreamName("backend-2".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }],
                },
            ],
        }];

        let router = Router::new(routes).unwrap();
        let (_, route) = router.match_request(None, "/app").expect("match");
        matches!(route.matcher, PathMatch::Prefix { .. });
    }
}
