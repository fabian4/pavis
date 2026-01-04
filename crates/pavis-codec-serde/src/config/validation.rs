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
    if config.upstreams.is_empty() {
        // It's technically allowed to have no upstreams, but let's warn or check consistency.
    }

    // 2. Cross-reference checks (Routes -> Upstreams)
    let upstream_names: HashSet<&str> = config.upstreams.iter().map(|u| u.name.as_str()).collect();

    for vhost in &config.routes {
        for route in &vhost.paths {
            for dest in &route.destinations {
                if !upstream_names.contains(dest.upstream.as_str()) {
                    return Err(anyhow::anyhow!(
                        "Route '{}' references unknown upstream '{}'",
                        matcher_path(&route.matcher),
                        dest.upstream
                    ));
                }
            }
        }
    }

    // 3. Retry Policy Validation (String -> Duration parsing check is already handled by humantime-serde in types.rs for deserialization,
    // but we can add extra checks if needed. Actually, per_try_timeout is already Duration in types.rs)
    for vhost in &config.routes {
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

    Ok(())
}

fn matcher_path(matcher: &Matcher) -> &str {
    match matcher {
        Matcher::Prefix { path } => path.as_str(),
        Matcher::Exact { path } => path.as_str(),
        Matcher::Regex { path } => path.as_str(),
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
            listeners: vec![],
            telemetry: Default::default(),
            upstreams: vec![],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    matcher: Matcher::Prefix {
                        path: "/".to_string(),
                    },
                    timeout: None,
                    retry: Some(RetryPolicy {
                        attempts: 1,
                        per_try_timeout: Duration::from_secs(1),
                        retry_on: vec![json!("5xx")],
                    }),
                    request_headers: None,
                    response_headers: None,
                    rewrite: None,
                    destinations: vec![],
                }],
            }],
        };
        assert!(validate(&mut config).is_ok());
    }

    #[test]
    fn validate_rejects_non_string_retry_on_values() {
        let mut config = SerdeConfig {
            listeners: vec![],
            telemetry: Default::default(),
            upstreams: vec![],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    matcher: Matcher::Prefix {
                        path: "/".to_string(),
                    },
                    timeout: None,
                    retry: Some(RetryPolicy {
                        attempts: 1,
                        per_try_timeout: Duration::from_secs(1),
                        retry_on: vec![json!(500)], // Number not allowed
                    }),
                    request_headers: None,
                    response_headers: None,
                    rewrite: None,
                    destinations: vec![],
                }],
            }],
        };
        assert!(validate(&mut config).is_err());
    }
}
