use crate::router::CompiledVirtualHost;
use pavis_core::{MatchType, Route, VirtualHost};

pub fn match_request<'a>(
    routes: &'a [CompiledVirtualHost],
    host_header: Option<&str>,
    uri_path: &str,
) -> Option<(&'a VirtualHost, &'a Route)> {
    for vhost in routes {
        if vhost.config.host == "*" || Some(vhost.config.host.as_str()) == host_header {
            for (i, route) in vhost.config.paths.iter().enumerate() {
                let is_match = match route.match_type {
                    MatchType::Prefix => uri_path.starts_with(&route.path),
                    MatchType::Exact => uri_path == route.path,
                    MatchType::Regex => vhost.regexes[i]
                        .as_ref()
                        .map(|re| re.is_match(uri_path))
                        .unwrap_or(false),
                };

                if is_match {
                    return Some((&vhost.config, route));
                }
            }
        }
    }
    None
}
