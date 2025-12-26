use crate::config::{MatchType, Route, VirtualHost};
use regex::Regex;

pub fn match_request<'a>(
    routes: &'a [VirtualHost],
    host_header: Option<&str>,
    uri_path: &str,
) -> Option<(&'a VirtualHost, &'a Route)> {
    for vhost in routes {
        if vhost.host == "*" || Some(vhost.host.as_str()) == host_header {
            for route in vhost.paths.iter() {
                let is_match = match route.match_type {
                    MatchType::Prefix => uri_path.starts_with(&route.path),
                    MatchType::Exact => uri_path == route.path,
                    MatchType::Regex => route
                        .compiled_regex
                        .as_ref()
                        .map(|re| re.is_match(uri_path))
                        .unwrap_or_else(|| {
                            Regex::new(&route.path)
                                .map(|re| re.is_match(uri_path))
                                .unwrap_or(false)
                        }),
                };

                if is_match {
                    return Some((vhost, route));
                }
            }
        }
    }
    None
}
