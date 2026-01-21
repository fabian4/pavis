use crate::runtime::{PathMatch, RewritePath, Route, RouteAction, Upstream, VirtualHost};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use super::headers::validate_headers;
use super::{CoreValidationError, CoreValidationResult};

static REGEX_CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();

pub(super) fn validate_routes(
    routes: &[VirtualHost],
    upstreams: &[Upstream],
) -> CoreValidationResult<()> {
    let upstream_names: HashSet<&str> = upstreams.iter().map(|u| u.name.0.as_str()).collect();

    for vhost in routes {
        let mut seen_routes: HashSet<(&str, &str)> = HashSet::new();
        for route in &vhost.paths {
            let (match_type, path) = match &route.matcher.path {
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
            if !seen_routes.insert((match_type, path.0.as_str())) {
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
                validate_regex(&path.0, &vhost.host.0)?;

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

pub(super) fn validate_regex_with_cache(
    cache: &Mutex<HashMap<String, Regex>>,
    path: &str,
    host: &str,
) -> CoreValidationResult<()> {
    let guard = match cache.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            drop(poisoned.into_inner());
            cache.clear_poison();
            return Err(CoreValidationError::RegexCachePoisoned);
        }
    };
    if guard.contains_key(path) {
        return Ok(());
    }
    drop(guard);

    let compiled = Regex::new(path).map_err(|e| CoreValidationError::InvalidRegex {
        host: host.to_string(),
        route: path.to_string(),
        error: e.to_string(),
    })?;

    let mut guard = match cache.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            drop(poisoned.into_inner());
            cache.clear_poison();
            return Err(CoreValidationError::RegexCachePoisoned);
        }
    };
    guard.entry(path.to_string()).or_insert(compiled);
    Ok(())
}

fn validate_regex(path: &str, host: &str) -> CoreValidationResult<()> {
    let cache = REGEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    validate_regex_with_cache(cache, path, host)
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
    match &route.matcher.path {
        PathMatch::Prefix { path } => path.0.clone(),
        PathMatch::Exact { path } => path.0.clone(),
        PathMatch::Regex { path } => path.0.clone(),
    }
}
