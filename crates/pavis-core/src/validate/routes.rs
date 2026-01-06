use crate::runtime::{PathMatch, RewritePath, Route, RouteAction, Upstream, VirtualHost};
use regex::Regex;
use std::collections::HashSet;

use super::headers::validate_headers;
use super::{CoreValidationError, CoreValidationResult};

pub(super) fn validate_routes(
    routes: &[VirtualHost],
    upstreams: &[Upstream],
) -> CoreValidationResult<()> {
    let upstream_names: HashSet<&str> = upstreams.iter().map(|u| u.name.0.as_str()).collect();

    for vhost in routes {
        let mut seen_routes = HashSet::new();
        for route in &vhost.paths {
            let (match_type, path) = match &route.matcher {
                PathMatch::Prefix { path } => ("prefix", path),
                PathMatch::Exact { path } => ("exact", path),
                PathMatch::Regex { path } => ("regex", path),
            };

            // Path Normalization Check (Skipped for Regex)
            if match_type != "regex"
                && (!path.0.starts_with('/') || (path.0.len() > 1 && path.0.ends_with('/')))
            {
                return Err(CoreValidationError::PathNotNormalized(path.0.clone()));
            }

            // Duplicate Route Detection
            if !seen_routes.insert((match_type, path.0.clone())) {
                return Err(CoreValidationError::DuplicateRoute {
                    host: vhost.host.0.clone(),
                    route: path.0.clone(),
                    match_type: match_type.to_string(),
                });
            }

            if match_type == "regex" {
                if path.0.len() > 2048 {
                    return Err(CoreValidationError::RegexTooLong {
                        route: path.0.clone(),
                    });
                }
                let _compiled =
                    Regex::new(&path.0).map_err(|e| CoreValidationError::InvalidRegex {
                        host: vhost.host.0.clone(),
                        route: path.0.clone(),
                        error: e.to_string(),
                    })?;

                // Constraint Check: Reject Rewrite configurations if PathMatch::Regex is used.
                if !matches!(route.rewrite.path, RewritePath::Disabled) {
                    return Err(CoreValidationError::RewriteRegexConflict(
                        path.0.clone(),
                        vhost.host.0.clone(),
                    ));
                }
            }

            validate_headers(
                &route.request_headers,
                &format!("Route '{}' request headers", path.0),
            )?;
            validate_headers(
                &route.response_headers,
                &format!("Route '{}' response headers", path.0),
            )?;

            validate_action(&route.action, route, vhost, &upstream_names)?;
        }
    }
    Ok(())
}

fn validate_action(
    action: &RouteAction,
    route: &Route,
    vhost: &VirtualHost,
    upstream_names: &HashSet<&str>,
) -> CoreValidationResult<()> {
    match action {
        RouteAction::Forward(destinations) => {
            if destinations.is_empty() {
                return Err(CoreValidationError::ForwardHasNoDestinations(
                    route_path(route),
                    vhost.host.0.clone(),
                ));
            }
            for dest in destinations {
                if !upstream_names.contains(dest.upstream.0.as_str()) {
                    return Err(CoreValidationError::UnknownDestination(
                        route_path(route),
                        vhost.host.0.clone(),
                        dest.upstream.0.clone(),
                    ));
                }
            }
        }
        RouteAction::Redirect { .. } | RouteAction::Direct { .. } => {}
    }
    Ok(())
}

fn route_path(route: &Route) -> String {
    match &route.matcher {
        PathMatch::Prefix { path } => path.0.clone(),
        PathMatch::Exact { path } => path.0.clone(),
        PathMatch::Regex { path } => path.0.clone(),
    }
}
