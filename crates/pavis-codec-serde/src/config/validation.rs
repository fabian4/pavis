//! Validation logic for serde-backed constraints.
//!
//! Canonical semantic validation now lives in `pavis-core::validate_runtime`.
//! This module should only contain validation logic that is specific to the
//! input format or user-friendly error reporting before conversion.

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use super::types::*;
use pavis_core::Discovery;

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

    validate_upstream_sni_auto_requires_dns_or_override(config)?;

    Ok(())
}

fn validate_upstream_sni_auto_requires_dns_or_override(config: &SerdeConfig) -> Result<()> {
    let upstreams = match config.upstreams.as_ref() {
        Some(upstreams) => upstreams,
        None => return Ok(()),
    };

    struct OverrideState {
        referenced: bool,
        missing_override: bool,
    }

    let mut needs_override: HashMap<&str, OverrideState> = HashMap::new();
    for upstream in upstreams {
        let tls = match upstream.tls.as_ref() {
            Some(tls) => tls,
            None => continue,
        };
        if matches!(tls.enabled, Some(false)) {
            continue;
        }

        let verify_cert = tls.verify_cert.unwrap_or(true);
        let verify_hostname = tls.verify_hostname.unwrap_or(true);
        if !verify_cert || !verify_hostname {
            continue;
        }

        let sni_auto = match tls.sni_mode {
            Some(SniMode::Auto) => true,
            Some(SniMode::Name) | Some(SniMode::Disabled) => false,
            None => tls.sni.is_none(),
        };
        if !sni_auto {
            continue;
        }

        let discovery = upstream.discovery.unwrap_or_default();
        let has_dns = matches!(discovery, Discovery::Logical | Discovery::Strict { .. });
        if !has_dns {
            needs_override.insert(
                upstream.name.as_str(),
                OverrideState {
                    referenced: false,
                    missing_override: false,
                },
            );
        }
    }

    if needs_override.is_empty() {
        return Ok(());
    }

    if let Some(routes) = &config.routes {
        for vhost in routes {
            for route in &vhost.paths {
                let rewrite_host = route
                    .rewrite
                    .as_ref()
                    .and_then(|rewrite| rewrite.host.as_ref())
                    .filter(|host| !host.trim().is_empty());
                if let RouteAction::Forward { destinations } = &route.action {
                    for dest in destinations {
                        if let Some(entry) = needs_override.get_mut(dest.upstream.as_str()) {
                            entry.referenced = true;
                            if rewrite_host.is_none() {
                                entry.missing_override = true;
                            }
                        }
                    }
                }
            }
        }
    }

    for (name, state) in needs_override {
        if !state.referenced || state.missing_override {
            return Err(anyhow::anyhow!(
                "upstream '{}' verify=full with sni=auto requires DNS endpoints or route host rewrite",
                name
            ));
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

    #[test]
    fn validate_rejects_auto_sni_full_verify_without_dns_or_host_rewrite() {
        let mut config = SerdeConfig {
            listeners: Some(vec![]),
            telemetry: None,
            upstreams: Some(vec![Upstream {
                id: None,
                name: "backend".to_string(),
                discovery: None,
                balancer: None,
                protocol: None,
                pool: None,
                tls: Some(UpstreamTlsConfig {
                    enabled: Some(true),
                    verify_hostname: Some(true),
                    verify_cert: Some(true),
                    sni: None,
                    sni_mode: Some(SniMode::Auto),
                    ca_bundle_path: None,
                    cert: None,
                }),
                circuit_breaker: None,
                health_check: None,
                endpoints: vec![Endpoint {
                    address: "127.0.0.1".to_string(),
                    port: 443,
                    weight: None,
                }],
            }]),
            routes: Some(vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    matcher: Some(Matcher::Prefix {
                        path: "/".to_string(),
                    }),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    principal: None,
                    rewrite: None,
                    action: RouteAction::Forward {
                        destinations: vec![WeightedDestination {
                            upstream: "backend".to_string(),
                            weight: 1,
                        }],
                    },
                }],
            }]),
        };
        let err = validate(&mut config).expect_err("expected validation error");
        assert!(
            err.to_string()
                .contains("verify=full with sni=auto requires DNS endpoints or route host rewrite")
        );
    }

    #[test]
    fn validate_allows_auto_sni_full_verify_with_host_rewrite() {
        let mut config = SerdeConfig {
            listeners: Some(vec![]),
            telemetry: None,
            upstreams: Some(vec![Upstream {
                id: None,
                name: "backend".to_string(),
                discovery: None,
                balancer: None,
                protocol: None,
                pool: None,
                tls: Some(UpstreamTlsConfig {
                    enabled: Some(true),
                    verify_hostname: Some(true),
                    verify_cert: Some(true),
                    sni: None,
                    sni_mode: Some(SniMode::Auto),
                    ca_bundle_path: None,
                    cert: None,
                }),
                circuit_breaker: None,
                health_check: None,
                endpoints: vec![Endpoint {
                    address: "127.0.0.1".to_string(),
                    port: 443,
                    weight: None,
                }],
            }]),
            routes: Some(vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    matcher: Some(Matcher::Prefix {
                        path: "/".to_string(),
                    }),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    principal: None,
                    rewrite: Some(RewritePolicy {
                        path: None,
                        host: Some("backend.local".to_string()),
                    }),
                    action: RouteAction::Forward {
                        destinations: vec![WeightedDestination {
                            upstream: "backend".to_string(),
                            weight: 1,
                        }],
                    },
                }],
            }]),
        };
        assert!(validate(&mut config).is_ok());
    }
}
