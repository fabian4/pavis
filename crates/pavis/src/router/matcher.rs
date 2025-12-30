use crate::router::CompiledVirtualHost;
use pavis_core::{MatchType, Route, VirtualHost};

#[allow(clippy::collapsible_if)]
pub(crate) fn match_request<'a>(
    routes: &'a [CompiledVirtualHost],
    host_header: Option<&str>,
    uri_path: &str,
) -> Option<(&'a VirtualHost, &'a Route)> {
    let normalized_host = host_header.map(normalize_host);
    let try_match = |vhost: &'a CompiledVirtualHost| -> Option<(&'a VirtualHost, &'a Route)> {
        if vhost.config.host == "*" || Some(vhost.config.host.as_str()) == normalized_host {
            for compiled in &vhost.routes {
                let route = &vhost.config.paths[compiled.index];
                let is_match = match route.match_type {
                    MatchType::Prefix => uri_path.starts_with(&route.path),
                    MatchType::Exact => uri_path == route.path,
                    MatchType::Regex => compiled
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
            if vhost.config.host != "*" && vhost.config.host == host {
                if let Some(found) = try_match(vhost) {
                    return Some(found);
                }
            }
        }
        for vhost in routes {
            if vhost.config.host == "*" {
                if let Some(found) = try_match(vhost) {
                    return Some(found);
                }
            }
        }
        return None;
    }

    for vhost in routes {
        if vhost.config.host == "*" {
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
    use pavis_core::{MatchType, Route, VirtualHost, WeightedDestination};
    use regex::Regex;

    fn compiled_vhost(host: &str, route: Route, regex: Option<Regex>) -> CompiledVirtualHost {
        let index = 0;
        CompiledVirtualHost {
            config: VirtualHost {
                host: host.to_string(),
                paths: vec![route],
            },
            routes: vec![CompiledRoute { index, regex }],
        }
    }

    #[test]
    fn matcher_respects_host() {
        let route = Route {
            match_type: MatchType::Exact,
            path: "/".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            destinations: vec![WeightedDestination {
                upstream: "backend".to_string(),
                weight: 1,
            }],
        };
        let routes = vec![compiled_vhost("example.com", route, None)];
        assert!(match_request(&routes, Some("other.com"), "/").is_none());
        assert!(match_request(&routes, Some("example.com"), "/").is_some());
    }

    #[test]
    fn matcher_uses_regex_when_configured() {
        let route = Route {
            match_type: MatchType::Regex,
            path: "^/items/[0-9]+$".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            destinations: vec![WeightedDestination {
                upstream: "backend".to_string(),
                weight: 1,
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
            match_type: MatchType::Exact,
            path: "/".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            destinations: vec![WeightedDestination {
                upstream: "backend".to_string(),
                weight: 1,
            }],
        };
        let routes = vec![compiled_vhost("example.com", route, None)];
        assert!(match_request(&routes, Some("example.com:8080"), "/").is_some());
    }

    #[test]
    fn matcher_normalizes_ipv6_host_header() {
        let route = Route {
            match_type: MatchType::Exact,
            path: "/".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            destinations: vec![WeightedDestination {
                upstream: "backend".to_string(),
                weight: 1,
            }],
        };
        let routes = vec![compiled_vhost("::1", route, None)];
        assert!(match_request(&routes, Some("[::1]:8080"), "/").is_some());
    }
}
