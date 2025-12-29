use crate::runtime::{MatchType, Route, Upstream, VirtualHost};
use regex::Regex;
use std::collections::HashSet;

use super::headers::validate_headers;
use super::{CoreValidationError, CoreValidationResult};

pub(super) fn validate_routes(
    routes: &[VirtualHost],
    upstreams: &[Upstream],
) -> CoreValidationResult<()> {
    let upstream_names: HashSet<&str> = upstreams.iter().map(|u| u.name.as_str()).collect();

    for vhost in routes {
        let mut seen_routes = HashSet::new();
        for route in &vhost.paths {
            // Path Normalization Check (Skipped for Regex)
            if route.match_type != MatchType::Regex
                && (!route.path.starts_with('/')
                    || (route.path.len() > 1 && route.path.ends_with('/')))
            {
                return Err(CoreValidationError::PathNotNormalized(route.path.clone()));
            }

            // Duplicate Route Detection
            let match_key = route.match_type;
            let normalized = &route.path;
            if !seen_routes.insert((match_key, normalized.clone())) {
                return Err(CoreValidationError::DuplicateRoute {
                    host: vhost.host.clone(),
                    route: route.path.clone(),
                    match_type: route.match_type,
                });
            }

            if route.match_type == MatchType::Regex {
                if route.path.len() > 2048 {
                    return Err(CoreValidationError::RegexTooLong {
                        route: route.path.clone(),
                    });
                }
                let _compiled =
                    Regex::new(&route.path).map_err(|e| CoreValidationError::InvalidRegex {
                        host: vhost.host.clone(),
                        route: route.path.clone(),
                        error: e.to_string(),
                    })?;
            }

            if let Some(headers) = &route.request_headers {
                validate_headers(headers, &format!("Route '{}' request headers", route.path))?;
            }
            if let Some(headers) = &route.response_headers {
                validate_headers(headers, &format!("Route '{}' response headers", route.path))?;
            }

            validate_destinations(route, vhost, &upstream_names)?;
        }
    }
    Ok(())
}

fn validate_destinations(
    route: &Route,
    vhost: &VirtualHost,
    upstream_names: &HashSet<&str>,
) -> CoreValidationResult<()> {
    for dest in &route.destinations {
        if !upstream_names.contains(dest.upstream.as_str()) {
            return Err(CoreValidationError::UnknownDestination(
                route.path.clone(),
                vhost.host.clone(),
                dest.upstream.clone(),
            ));
        }
        if dest.weight == 0 {
            return Err(CoreValidationError::DestinationWeightZero(
                route.path.clone(),
                vhost.host.clone(),
                dest.upstream.clone(),
            ));
        }
    }
    Ok(())
}
