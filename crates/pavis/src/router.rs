//! Router module: Request matching and routing logic.
//!
//! # Architectural Invariants
//!
//! 1. **Deterministic Matching**: Route matching order is significant and must be deterministic.
//! 2. **Pre-compiled Regex**: All regular expressions must be compiled at initialization time, never during request handling.
//! 3. **Read-Only**: The router state is immutable after initialization.

use anyhow::{Context, Result};
use pavis_core::{MatchType, Route, VirtualHost};
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
                let regex = if route.match_type == MatchType::Regex {
                    Some(Regex::new(&route.path).with_context(|| {
                        format!("Failed to compile regex for path: {}", route.path)
                    })?)
                } else {
                    None
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
    use pavis_core::{MatchType, Route, VirtualHost, WeightedDestination};

    fn create_routes() -> Vec<VirtualHost> {
        vec![
            VirtualHost {
                host: "example.com".to_string(),
                paths: vec![
                    Route {
                        match_type: MatchType::Exact,
                        path: "/exact".to_string(),
                        timeout_ms: None,
                        retry_policy: None,
                        request_headers: None,
                        response_headers: None,
                        rewrite: None,
                        destinations: vec![WeightedDestination {
                            upstream: "backend-1".to_string(),
                            weight: 1,
                        }],
                    },
                    Route {
                        match_type: MatchType::Prefix,
                        path: "/api".to_string(),
                        timeout_ms: None,
                        retry_policy: None,
                        request_headers: None,
                        response_headers: None,
                        rewrite: None,
                        destinations: vec![WeightedDestination {
                            upstream: "backend-1".to_string(),
                            weight: 1,
                        }],
                    },
                ],
            },
            VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Prefix,
                    path: "/public".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    rewrite: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend-2".to_string(),
                        weight: 1,
                    }],
                }],
            },
        ]
    }

    #[test]
    fn test_invalid_regex_compilation() {
        let routes = vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Regex,
                path: "[unclosed".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
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
        assert_eq!(vhost.host, "example.com");
        assert_eq!(route.path, "/exact");
    }

    #[test]
    fn test_find_route_prefix_match() {
        let router = Router::new(create_routes()).unwrap();
        let (vhost, route) = router
            .match_request(Some("example.com"), "/api/v1/users")
            .expect("Should match");
        assert_eq!(vhost.host, "example.com");
        assert_eq!(route.path, "/api");
    }

    #[test]
    fn test_find_route_wildcard_host() {
        let router = Router::new(create_routes()).unwrap();
        let (vhost, route) = router
            .match_request(Some("any.com"), "/public/stuff")
            .expect("Should match");
        assert_eq!(vhost.host, "*");
        assert_eq!(route.path, "/public");
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
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Regex,
                path: r"^/api/v[0-9]+/users/\d+$".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: "backend".to_string(),
                    weight: 1,
                }],
            }],
        }];

        let router = Router::new(routes).unwrap();

        let result = router.match_request(None, "/api/v1/users/123");
        assert!(result.is_some());
        let (_, route) = result.unwrap();
        assert_eq!(route.match_type, MatchType::Regex);

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
            host: "*".to_string(),
            paths: vec![
                Route {
                    match_type: MatchType::Prefix,
                    path: "/app".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    rewrite: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend-1".to_string(),
                        weight: 1,
                    }],
                },
                Route {
                    match_type: MatchType::Exact,
                    path: "/app".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    rewrite: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend-2".to_string(),
                        weight: 1,
                    }],
                },
            ],
        }];

        let router = Router::new(routes).unwrap();
        let (_, route) = router.match_request(None, "/app").expect("match");
        assert_eq!(route.match_type, MatchType::Prefix);
    }
}
