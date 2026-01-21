//! Validation logic for serde-backed constraints.
//!
//! Canonical semantic validation now lives in `pavis-core::validate_runtime`.
//! This module should only contain validation logic that is specific to the
//! input format or user-friendly error reporting before conversion.

use super::types::*;
use anyhow::Result;

/// Perform format-specific validation on the configuration.
pub fn validate(config: &mut SerdeConfig) -> Result<()> {
    // 1. Basic field checks
    if config.upstreams.as_ref().map(Vec::is_empty).unwrap_or(true) {
        // It's technically allowed to have no upstreams, but let's warn or check consistency.
    }

    // 2. Retry Policy Validation (String -> Duration parsing check is already handled by humantime-serde in types.rs for deserialization,
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

    validate_phase6_policies(config)?;

    Ok(())
}

fn validate_phase6_policies(config: &SerdeConfig) -> Result<()> {
    let upstreams = match config.upstreams.as_ref() {
        Some(upstreams) => upstreams,
        None => return Ok(()),
    };

    for upstream in upstreams {
        if let Some(circuit_breaker) = &upstream.circuit_breaker {
            if circuit_breaker.max_retries.is_some() {
                return Err(anyhow::anyhow!(
                    "upstream '{}' sets circuit_breaker.max_retries (unsupported)",
                    upstream.name
                ));
            }
            if circuit_breaker.max_connections == 0 || circuit_breaker.max_pending_requests == 0 {
                return Err(anyhow::anyhow!(
                    "upstream '{}' circuit_breaker limits must be > 0",
                    upstream.name
                ));
            }
        }

        if let Some(outlier) = &upstream.outlier_detection {
            if outlier.consecutive_errors == 0 {
                return Err(anyhow::anyhow!(
                    "upstream '{}' outlier_detection.consecutive_errors must be > 0",
                    upstream.name
                ));
            }
            if outlier.eject_duration.as_millis() == 0 {
                return Err(anyhow::anyhow!(
                    "upstream '{}' outlier_detection.eject_duration must be > 0",
                    upstream.name
                ));
            }
        }

        if let Some(health_check) = &upstream.health_check {
            if health_check.healthy_threshold != 1 || health_check.unhealthy_threshold != 1 {
                return Err(anyhow::anyhow!(
                    "upstream '{}' health_check thresholds must be 1",
                    upstream.name
                ));
            }
            if health_check.path.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "upstream '{}' health_check.path cannot be empty",
                    upstream.name
                ));
            }
            if !health_check.path.starts_with('/') || health_check.path.contains(' ') {
                return Err(anyhow::anyhow!(
                    "upstream '{}' health_check.path must start with '/' and contain no spaces",
                    upstream.name
                ));
            }
            if health_check.interval.as_millis() == 0 {
                return Err(anyhow::anyhow!(
                    "upstream '{}' health_check.interval must be > 0",
                    upstream.name
                ));
            }
            if let Some(timeout) = health_check.timeout {
                if timeout.as_millis() == 0 {
                    return Err(anyhow::anyhow!(
                        "upstream '{}' health_check.timeout must be > 0",
                        upstream.name
                    ));
                }
                if timeout > health_check.interval {
                    return Err(anyhow::anyhow!(
                        "upstream '{}' health_check.timeout must be <= health_check.interval",
                        upstream.name
                    ));
                }
            }
        }
    }

    Ok(())
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
                    matcher: Some(Matcher {
                        path: PathMatcher::Prefix {
                            path: "/".to_string(),
                        },
                        method: None,
                        headers: None,
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
            shutdown: None,
            admin: None,
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
                    matcher: Some(Matcher {
                        path: PathMatcher::Prefix {
                            path: "/".to_string(),
                        },
                        method: None,
                        headers: None,
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
            shutdown: None,
            admin: None,
        };
        assert!(validate(&mut config).is_err());
    }

    // Semantic SNI auto checks now live in pavis-core.
}
