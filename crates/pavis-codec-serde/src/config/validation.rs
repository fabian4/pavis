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

    // 2. Retry Policy Validation
    if let Some(routes) = &config.routes {
        for vhost in routes {
            for route in &vhost.paths {
                if let Some(retry) = &route.retry {
                    // Validate retryable_reasons are valid strings
                    for reason in &retry.retryable_reasons {
                        if reason.is_empty() {
                            return Err(anyhow::anyhow!(
                                "Retry retryable_reasons cannot contain empty strings"
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
                        methods: None,
                        headers: None,
                    }),
                    timeout: None,
                    retry: Some(RetryPolicy {
                        max_attempts: 1,
                        retryable_reasons: vec!["status_code".to_string()],
                        retryable_status_codes: Some(vec![500, 502, 503, 504]),
                        backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
                        retry_non_idempotent: false,
                        fail_on_non_replayable_retry: false,
                        max_request_body_buffer_bytes: 1_048_576,
                        per_try: Some(Duration::from_secs(1)),
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
            features: None,
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
                        methods: None,
                        headers: None,
                    }),
                    timeout: None,
                    retry: Some(RetryPolicy {
                        max_attempts: 1,
                        retryable_reasons: vec!["".to_string()], // Empty string not allowed
                        retryable_status_codes: Some(vec![500]),
                        backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
                        retry_non_idempotent: false,
                        fail_on_non_replayable_retry: false,
                        max_request_body_buffer_bytes: 1_048_576,
                        per_try: Some(Duration::from_secs(1)),
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
            features: None,
        };
        assert!(validate(&mut config).is_err());
    }

    // Semantic SNI auto checks now live in pavis-core.

    #[test]
    fn validate_circuit_breaker_phase6() {
        // 1. max_retries unsupported
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                circuit_breaker: Some(CircuitBreaker {
                    max_connections: 1,
                    max_pending_requests: 1,
                    max_retries: Some(3),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());

        // 2. max_connections == 0
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                circuit_breaker: Some(CircuitBreaker {
                    max_connections: 0,
                    max_pending_requests: 1,
                    max_retries: None,
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());
    }

    #[test]
    fn validate_outlier_detection_phase6() {
        // 1. consecutive_errors == 0
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                outlier_detection: Some(OutlierDetection {
                    consecutive_errors: 0,
                    eject_duration: Duration::from_secs(10),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());

        // 2. eject_duration == 0
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                outlier_detection: Some(OutlierDetection {
                    consecutive_errors: 1,
                    eject_duration: Duration::from_secs(0),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());
    }

    #[test]
    fn validate_health_check_phase6() {
        // 1. thresholds != 1
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                health_check: Some(HealthCheck {
                    path: "/".to_string(),
                    interval: Duration::from_secs(5),
                    timeout: None,
                    healthy_threshold: 2,
                    unhealthy_threshold: 1,
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());

        // 2. path empty
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                health_check: Some(HealthCheck {
                    path: "".to_string(),
                    interval: Duration::from_secs(5),
                    timeout: None,
                    healthy_threshold: 1,
                    unhealthy_threshold: 1,
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());

        // 3. invalid path (no slash)
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                health_check: Some(HealthCheck {
                    path: "foo".to_string(),
                    interval: Duration::from_secs(5),
                    timeout: None,
                    healthy_threshold: 1,
                    unhealthy_threshold: 1,
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());

        // 4. interval == 0
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                health_check: Some(HealthCheck {
                    path: "/".to_string(),
                    interval: Duration::from_secs(0),
                    timeout: None,
                    healthy_threshold: 1,
                    unhealthy_threshold: 1,
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());

        // 5. timeout > interval
        let mut config = SerdeConfig {
            upstreams: Some(vec![Upstream {
                name: "u1".to_string(),
                health_check: Some(HealthCheck {
                    path: "/".to_string(),
                    interval: Duration::from_secs(5),
                    timeout: Some(Duration::from_secs(6)),
                    healthy_threshold: 1,
                    unhealthy_threshold: 1,
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(validate(&mut config).is_err());
    }
}
