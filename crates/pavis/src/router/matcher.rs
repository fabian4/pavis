use crate::router::CompiledVirtualHost;
use pavis_core::{PathMatch, Route, VirtualHost};

#[allow(clippy::collapsible_if)]
pub(crate) fn match_request<'a>(
    routes: &'a [CompiledVirtualHost],
    host_header: Option<&str>,
    uri_path: &str,
) -> Option<(&'a VirtualHost, &'a Route)> {
    let normalized_host = host_header.map(normalize_host);
    let try_match = |vhost: &'a CompiledVirtualHost| -> Option<(&'a VirtualHost, &'a Route)> {
        if vhost.config.host.0 == "*" || Some(vhost.config.host.0.as_str()) == normalized_host {
            for compiled in &vhost.routes {
                let route = &vhost.config.paths[compiled.index];
                let is_match = match &route.matcher {
                    PathMatch::Prefix { path } => uri_path.starts_with(&path.0),
                    PathMatch::Exact { path } => uri_path == path.0,
                    PathMatch::Regex { .. } => compiled
                        .regex
                        .as_ref()
                        .map(|re| re.is_match(uri_path))
                        .unwrap_or(false),
                };

                if is_match {
                    return Some((&vhost.config, route));
                }
            }
        }
        None
    };

    if let Some(host) = normalized_host {
        for vhost in routes {
            if vhost.config.host.0 != "*" && vhost.config.host.0 == host {
                if let Some(found) = try_match(vhost) {
                    return Some(found);
                }
            }
        }
        for vhost in routes {
            if vhost.config.host.0 == "*" {
                if let Some(found) = try_match(vhost) {
                    return Some(found);
                }
            }
        }
        return None;
    }

    for vhost in routes {
        if vhost.config.host.0 == "*" {
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
    use super::match_request;
    use crate::router::{CompiledRoute, CompiledVirtualHost};
    use pavis_core::{
        Destination, HeadersPolicy, Host, Path, PathMatch, RetryPolicy, Rewrite, RewriteHost,
        RewritePath, Route, Timeout, UpstreamName, VirtualHost, Weight,
    };
    use regex::Regex;
    use std::num::NonZeroU16;

    fn compiled_vhost(host: &str, route: Route, regex: Option<Regex>) -> CompiledVirtualHost {
        let index = 0;
        CompiledVirtualHost {
            config: VirtualHost {
                host: Host(host.to_string()),
                paths: vec![route],
            },
            routes: vec![CompiledRoute { index, regex }],
        }
    }

    #[test]
    fn matcher_respects_host() {
        let route = Route {
            matcher: PathMatch::Exact {
                path: Path("/".to_string()),
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
        };
        let routes = vec![compiled_vhost("example.com", route, None)];
        assert!(match_request(&routes, Some("other.com"), "/").is_none());
        assert!(match_request(&routes, Some("example.com"), "/").is_some());
    }

    #[test]
    fn matcher_uses_regex_when_configured() {
        let route = Route {
            matcher: PathMatch::Regex {
                path: Path("^/items/[0-9]+$".to_string()),
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
        };
        let regex = Regex::new("^/items/[0-9]+$").unwrap();
        let routes = vec![compiled_vhost("*", route, Some(regex))];
        assert!(match_request(&routes, None, "/items/123").is_some());
        assert!(match_request(&routes, None, "/items/abc").is_none());
    }

    #[test]
    fn matcher_strips_port_from_host_header() {
        let route = Route {
            matcher: PathMatch::Exact {
                path: Path("/".to_string()),
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
        };
        let routes = vec![compiled_vhost("example.com", route, None)];
        assert!(match_request(&routes, Some("example.com:8080"), "/").is_some());
    }

    #[test]
    fn matcher_normalizes_ipv6_host_header() {
        let route = Route {
            matcher: PathMatch::Exact {
                path: Path("/".to_string()),
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
        };
        let routes = vec![compiled_vhost("::1", route, None)];
        assert!(match_request(&routes, Some("[::1]:8080"), "/").is_some());
    }
}
