use crate::router::{CompiledVirtualHost, RouteZone, Router};
use pavis_core::{PathMatch, Route, VirtualHost};

#[allow(clippy::collapsible_if)]
pub(crate) fn match_request<'a>(
    router: &'a Router,
    host_header: Option<&str>,
    uri_path: &str,
) -> Option<(&'a VirtualHost, &'a Route)> {
    let normalized_host = host_header.map(normalize_host);

    let try_match = |vhost: &'a CompiledVirtualHost| -> Option<(&'a VirtualHost, &'a Route)> {
        for zone in &vhost.zones {
            match zone {
                RouteZone::Linear(routes) => {
                    for compiled in routes {
                        let route = &vhost.config.paths[compiled.index];
                        let is_match = match &route.matcher {
                            PathMatch::Prefix { path } => uri_path.starts_with(&path.0),
                            PathMatch::Exact { path } => uri_path == path.0,
                            PathMatch::Regex { .. } => compiled
                                .regex
                                .as_ref()
                                .map(|re| re.is_match(uri_path))
                                .unwrap_or(false),
                            #[allow(unreachable_patterns)]
                            &_ => false,
                        };

                        if is_match {
                            return Some((&vhost.config, route));
                        }
                    }
                }
                RouteZone::ExactMap(map) => {
                    if let Some(compiled) = map.get(uri_path) {
                        return Some((&vhost.config, &vhost.config.paths[compiled.index]));
                    }
                }
            }
        }
        None
    };

    // 1. Try exact host match
    if let Some(host) = normalized_host {
        if let Some(vhost) = router.exact_hosts.get(host) {
            if let Some(found) = try_match(vhost) {
                return Some(found);
            }
        }
    }

    // 2. Try wildcard host matches (order preserved from config)
    for vhost in &router.wildcard_hosts {
        let pattern = &vhost.config.host.0;
        let is_match = if pattern == "*" {
            true
        } else if let Some(suffix) = pattern.strip_prefix("*.") {
            normalized_host.is_some_and(|h| {
                h.ends_with(suffix)
                    && h.len() > suffix.len()
                    && h.as_bytes()[h.len() - suffix.len() - 1] == b'.'
            })
        } else if let Some(prefix) = pattern.strip_suffix(".*") {
            normalized_host.is_some_and(|h| {
                h.starts_with(prefix)
                    && h.len() > prefix.len()
                    && h.as_bytes()[prefix.len()] == b'.'
            })
        } else {
            normalized_host.is_some_and(|h| h == pattern)
        };

        if is_match {
            if let Some(found) = try_match(vhost) {
                return Some(found);
            }
        }
    }

    None
}

#[allow(clippy::collapsible_if)]
fn normalize_host(host: &str) -> &str {
    if let Some(stripped) = host.strip_prefix('[') {
        if let Some(end) = stripped.find(']') {
            return &stripped[..end];
        }
    }
    if let Some((host_only, _port)) = host.split_once(':') {
        return host_only;
    }
    host
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::CompiledRoute;
    use pavis_core::{
        Destination, HeadersPolicy, Host, Path, PathMatch, RetryPolicy, Rewrite, RewriteHost,
        RewritePath, Route, RouteAction, Timeout, Weight,
    };
    use std::collections::HashMap;
    use std::num::NonZeroU16;

    #[test]
    fn test_normalize_host() {
        assert_eq!(normalize_host("example.com"), "example.com");
        assert_eq!(normalize_host("example.com:8080"), "example.com");
        assert_eq!(normalize_host("[::1]"), "::1");
        assert_eq!(normalize_host("[::1]:8080"), "::1");
        assert_eq!(normalize_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(normalize_host("foo.bar.com:443"), "foo.bar.com");
    }

    fn make_route() -> Route {
        Route {
            matcher: PathMatch::Prefix {
                path: Path("/".to_string()),
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
        }
    }

    fn make_vhost(host: &str) -> CompiledVirtualHost {
        CompiledVirtualHost {
            config: VirtualHost {
                host: Host(host.to_string()),
                paths: vec![make_route()],
            },
            zones: vec![RouteZone::Linear(vec![CompiledRoute {
                index: 0,
                regex: None,
            }])],
        }
    }

    #[test]
    fn test_match_wildcard_host_patterns() {
        let suffix_vhost = make_vhost("*.example.com");
        let prefix_vhost = make_vhost("app.*");

        let router = Router {
            exact_hosts: HashMap::new(),
            wildcard_hosts: vec![suffix_vhost, prefix_vhost],
        };

        // Suffix match
        let (v, _) = match_request(&router, Some("foo.example.com"), "/").unwrap();
        assert_eq!(v.host.0, "*.example.com");

        // Prefix match
        let (v, _) = match_request(&router, Some("app.internal"), "/").unwrap();
        assert_eq!(v.host.0, "app.*");

        // No match
        assert!(match_request(&router, Some("other.com"), "/").is_none());
        assert!(match_request(&router, Some("example.com"), "/").is_none()); // strict suffix check in code? "normalized_host.is_some_and(|h| h.ends_with(suffix))"
        // If pattern is "*.example.com", suffix is ".example.com". "example.com" does not end with ".example.com". Correct.
    }

    #[test]
    fn test_match_exact_linear() {
        let vhost = CompiledVirtualHost {
            config: VirtualHost {
                host: Host("*".to_string()),
                paths: vec![Route {
                    matcher: PathMatch::Exact {
                        path: Path("/exact".to_string()),
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
                        upstream: pavis_core::UpstreamName("u".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }]),
                }],
            },
            zones: vec![RouteZone::Linear(vec![CompiledRoute {
                index: 0,
                regex: None,
            }])],
        };

        let router = Router {
            exact_hosts: HashMap::new(),
            wildcard_hosts: vec![vhost],
        };

        let (_, res) = match_request(&router, None, "/exact").unwrap();
        assert!(matches!(res.matcher, PathMatch::Exact { .. }));

        let res_miss = match_request(&router, None, "/exact/more");
        assert!(res_miss.is_none());
    }
}
