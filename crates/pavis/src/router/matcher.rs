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
    let wildcard_exact = normalized_host
        .and_then(|host| router.wildcard_exact.get(host))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let wildcard_all = router.wildcard_all.as_slice();

    let mut exact_index = 0;
    let mut all_index = 0;

    while exact_index < wildcard_exact.len() || all_index < wildcard_all.len() {
        let next = match (wildcard_exact.get(exact_index), wildcard_all.get(all_index)) {
            (Some(&exact), Some(&all)) => {
                if exact <= all {
                    exact_index += 1;
                    exact
                } else {
                    all_index += 1;
                    all
                }
            }
            (Some(&exact), None) => {
                exact_index += 1;
                exact
            }
            (None, Some(&all)) => {
                all_index += 1;
                all
            }
            (None, None) => break,
        };

        if let Some(vhost) = router.wildcard_hosts.get(next) {
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
                    request_headers: HeadersPolicy::Disabled,
                    response_headers: HeadersPolicy::Disabled,
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
            wildcard_exact: HashMap::new(),
            wildcard_all: vec![0],
        };

        let (_, res) = match_request(&router, None, "/exact").unwrap();
        assert!(matches!(res.matcher, PathMatch::Exact { .. }));

        let res_miss = match_request(&router, None, "/exact/more");
        assert!(res_miss.is_none());
    }
}
