//! Validation logic for serde-backed constraints.
//!
//! Canonical semantic validation now lives in `pavis-core::validate_runtime`.
//! This module should only contain validation logic that is specific to the
//! input format or user-friendly error reporting before conversion.

use anyhow::Result;
use std::collections::HashSet;

use super::types::*;

/// Perform format-specific validation on the configuration.
pub fn validate(config: &mut SerdeConfig) -> Result<()> {
    // 1. Basic field checks
    if config.upstreams.as_ref().map(Vec::is_empty).unwrap_or(true) {
        // It's technically allowed to have no upstreams, but let's warn or check consistency.
    }

    // 2. Cross-reference checks (Routes -> Upstreams)
    let upstream_names: HashSet<&str> = config
        .upstreams
        .as_ref()
        .map(|u| u.iter().map(|u| u.name.as_str()).collect())
        .unwrap_or_default();

    if let Some(routes) = &config.routes {
        for vhost in routes {
            for route in &vhost.paths {
                if let RouteAction::Forward { destinations } = &route.action {
                    for dest in destinations {
                        if !upstream_names.contains(dest.upstream.as_str()) {
                            return Err(anyhow::anyhow!(
                                "Route '{}' references unknown upstream '{}'",
                                matcher_path(route.matcher.as_ref()),
                                dest.upstream
                            ));
                        }
                    }
                }
            }
        }
    }

    // 3. Retry Policy Validation (String -> Duration parsing check is already handled by humantime-serde in types.rs for deserialization,
    // but we can add extra checks if needed. Actually, per_try_timeout is already Duration in types.rs)
    if let Some(routes) = &config.routes {
        for vhost in routes {
            for route in &vhost.paths {
                if let Some(retry) = &route.retry {
                    // Validate retry_on conditions are strings (serde_json::Value)
                    for cond in &retry.retry_on {
                        if !cond.is_string() {
                            return Err(anyhow::anyhow!(
                                "Retry condition must be a string, found: {:?}",
                                cond
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn matcher_path(matcher: Option<&Matcher>) -> &str {
    match matcher {
        None => "<missing matcher>",
        Some(Matcher::Prefix { path }) => path.as_str(),
        Some(Matcher::Exact { path }) => path.as_str(),
        Some(Matcher::Regex { path }) => path.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn validate_allows_string_retry_on_values() {
        let mut config = SerdeConfig {
            listeners: Some(vec![]),
            telemetry: None,
            upstreams: Some(vec![]),
            routes: Some(vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    matcher: Some(Matcher::Prefix {
                        path: "/".to_string(),
                    }),
                    timeout: None,
                    retry: Some(RetryPolicy {
                        attempts: 1,
                        per_try_timeout: Duration::from_secs(1),
                        retry_on: vec![json!("5xx")],
                    }),
                    request_headers: None,
                    response_headers: None,
                    principal: None,
                    rewrite: None,
                    action: RouteAction::Forward {
                        destinations: vec![],
                    },
                }],
            }]),
        };
        assert!(validate(&mut config).is_ok());
    }

    #[test]
    fn validate_rejects_non_string_retry_on_values() {
        let mut config = SerdeConfig {
            listeners: Some(vec![]),
            telemetry: None,
            upstreams: Some(vec![]),
            routes: Some(vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    matcher: Some(Matcher::Prefix {
                        path: "/".to_string(),
                    }),
                    timeout: None,
                    retry: Some(RetryPolicy {
                        attempts: 1,
                        per_try_timeout: Duration::from_secs(1),
                        retry_on: vec![json!(500)], // Number not allowed
                    }),
                    request_headers: None,
                    response_headers: None,
                    principal: None,
                    rewrite: None,
                    action: RouteAction::Forward {
                        destinations: vec![],
                    },
                }],
            }]),
        };
        assert!(validate(&mut config).is_err());
    }
}
