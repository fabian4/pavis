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
    for vhost in &router.wildcard_hosts {
        if vhost.config.host.0 == "*" || Some(vhost.config.host.0.as_str()) == normalized_host {
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
