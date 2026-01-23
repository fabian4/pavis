//! Regex validation and compilation at runtime apply time (boring reload contract)
//!
//! This module enforces regex syntax validation and compilation limits at config apply time,
//! ensuring deterministic ACCEPT/REJECT behavior before the config becomes active.

use pavis_core::RuntimeConfig;
use pavis_core::limits::RegexLimits;
use regex::bytes::Regex;
use std::collections::HashMap;
use std::sync::Arc;

/// Compiled regex pattern with metadata for runtime use
#[derive(Debug, Clone)]
pub struct CompiledRegex {
    pub pattern: String,
    pub compiled: Arc<Regex>,
}

impl CompiledRegex {
    /// Evaluate regex against header value (with input length limit)
    ///
    /// # Deterministic Behavior
    /// - Input exceeds `input_max_bytes` → return false (non-match, not error)
    pub fn is_match(&self, input: &[u8], max_len: usize) -> bool {
        if input.len() > max_len {
            return false; // Deterministic non-match
        }
        self.compiled.is_match(input)
    }
}

/// Validate and compile all regex patterns in config
///
/// # Enforcement Point
/// This is the ONLY place where regex syntax and compilation limits are enforced.
/// Codec performs ONLY static byte length checks.
///
/// # Deterministic Behavior
/// - Invalid syntax → REJECT at apply with error
/// - Compilation exceeds size_limit → REJECT at apply with error
/// - Too many regexes per route/config → REJECT at apply with error
///
/// # Guarantee
/// If this function returns Ok, all regexes are compilable and within limits.
pub fn validate_and_compile_regexes(
    config: &RuntimeConfig,
    limits: &RegexLimits,
) -> Result<HashMap<String, CompiledRegex>, String> {
    let mut compiled = HashMap::new();
    let mut total_regex_count = 0;

    for (route_idx, route) in config.routes.iter().enumerate() {
        // Count regexes in this route's matcher predicates
        let route_regex_count = count_regexes_in_route(route);

        // Check per-route limit
        if route_regex_count > limits.max_regex_per_route as usize {
            return Err(format!(
                "routes[{}]: too many regexes ({} exceeds limit of {})",
                route_idx, route_regex_count, limits.max_regex_per_route
            ));
        }

        total_regex_count += route_regex_count;

        // Collect and compile patterns from route matchers
        for (path_idx, path) in route.paths.iter().enumerate() {
            collect_and_compile_from_matcher(
                &path.matcher.headers,
                route_idx,
                path_idx,
                limits,
                &mut compiled,
            )?;
        }
    }

    // Check global limit
    if total_regex_count > limits.max_regex_per_config as usize {
        return Err(format!(
            "routes[*]: too many regexes total ({} exceeds limit of {})",
            total_regex_count, limits.max_regex_per_config
        ));
    }

    Ok(compiled)
}

/// Count regex patterns in a single route (all paths)
fn count_regexes_in_route(route: &pavis_core::VirtualHost) -> usize {
    route
        .paths
        .iter()
        .map(|path| count_regexes_in_headers(&path.matcher.headers))
        .sum()
}

/// Count regex patterns in header predicates
fn count_regexes_in_headers(headers: &pavis_core::HeaderPredicates) -> usize {
    match headers {
        pavis_core::HeaderPredicates::None => 0,
        pavis_core::HeaderPredicates::Some(predicates) => predicates
            .iter()
            .filter(|pred| matches!(pred.matcher, pavis_core::HeaderMatch::Regex(_)))
            .count(),
        #[allow(unreachable_patterns)]
        _ => 0,
    }
}

/// Collect and compile regex patterns from header predicates
fn collect_and_compile_from_matcher(
    headers: &pavis_core::HeaderPredicates,
    route_idx: usize,
    path_idx: usize,
    limits: &RegexLimits,
    compiled: &mut HashMap<String, CompiledRegex>,
) -> Result<(), String> {
    match headers {
        pavis_core::HeaderPredicates::None => Ok(()),
        pavis_core::HeaderPredicates::Some(predicates) => {
            for (header_idx, pred) in predicates.iter().enumerate() {
                if let pavis_core::HeaderMatch::Regex(pattern) = &pred.matcher {
                    compile_regex_pattern(
                        pattern.as_str(),
                        &pred.name,
                        route_idx,
                        path_idx,
                        header_idx,
                        limits,
                        compiled,
                    )?;
                }
            }
            Ok(())
        }
        #[allow(unreachable_patterns)]
        _ => Ok(()),
    }
}

/// Compile a single regex pattern with size limits
fn compile_regex_pattern(
    pattern: &str,
    header_name: &str,
    route_idx: usize,
    path_idx: usize,
    header_idx: usize,
    limits: &RegexLimits,
    compiled: &mut HashMap<String, CompiledRegex>,
) -> Result<(), String> {
    // Skip if already compiled (deduplication)
    if compiled.contains_key(pattern) {
        return Ok(());
    }

    // Syntax validation
    let _syntax_check = Regex::new(pattern).map_err(|e| {
        format!(
            "routes[{}].paths[{}].matcher.headers[{}] ('{}'): invalid regex syntax: {}",
            route_idx, path_idx, header_idx, header_name, e
        )
    })?;

    // Compilation with size limit
    let regex_with_limit = regex::bytes::RegexBuilder::new(pattern)
        .size_limit(limits.size_limit_bytes as usize)
        .build()
        .map_err(|e| {
            format!(
                "routes[{}].paths[{}].matcher.headers[{}] ('{}'): regex compilation failed (exceeds size limit): {}",
                route_idx, path_idx, header_idx, header_name, e
            )
        })?;

    compiled.insert(
        pattern.to_string(),
        CompiledRegex {
            pattern: pattern.to_string(),
            compiled: Arc::new(regex_with_limit),
        },
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::CompactString;
    use pavis_core::{
        AccessLogPolicy, HeaderMatch, HeaderPredicate, HeaderPredicates, Host, LogLevel,
        MethodPredicate, Metrics, Path, PathMatch, Route, RouteMatcher, ServiceName, Telemetry,
        TracingPolicy, VirtualHost,
    };

    fn default_telemetry() -> Telemetry {
        Telemetry {
            level: LogLevel::Info,
            pingora: LogLevel::Warn,
            service_name: ServiceName("test".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: TracingPolicy::Disabled,
        }
    }

    fn default_listener() -> pavis_core::Listener {
        use pavis_core::{ListenerName, TlsConfig, WorkerCount};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        pavis_core::ListenerBuilder::new()
            .name(ListenerName("test".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
            .workers(WorkerCount::Auto)
            .tls(TlsConfig::Disabled)
            .build()
            .unwrap()
    }

    fn make_test_route_with_regex(pattern: &str) -> VirtualHost {
        VirtualHost {
            host: Host("example.com".to_string()),
            paths: vec![Route {
                matcher: RouteMatcher {
                    path: PathMatch::Prefix {
                        path: Path("/".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::Some(vec![HeaderPredicate {
                        name: CompactString::new("x-version"),
                        matcher: HeaderMatch::Regex(CompactString::new(pattern)),
                    }]),
                },
                timeout: pavis_core::Timeout::Disabled,
                retry: pavis_core::RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: pavis_core::Rewrite {
                    path: pavis_core::RewritePath::Disabled,
                    host: pavis_core::RewriteHost::Disabled,
                },
                action: pavis_core::RouteAction::Direct {
                    status: 200,
                    body: "ok".to_string(),
                },
            }],
        }
    }

    #[test]
    fn validate_and_compile_valid_regex() {
        let route = make_test_route_with_regex("^v[0-9]+$");
        let config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(default_telemetry())
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(default_listener())
            .add_route(route)
            .build()
            .unwrap();

        let limits = RegexLimits::default();
        let result = validate_and_compile_regexes(&config, &limits);
        assert!(result.is_ok());

        let compiled = result.unwrap();
        assert_eq!(compiled.len(), 1);
        assert!(compiled.contains_key("^v[0-9]+$"));
    }

    #[test]
    fn validate_rejects_invalid_regex_syntax() {
        let route = make_test_route_with_regex("[unclosed");
        let config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(default_telemetry())
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(default_listener())
            .add_route(route)
            .build()
            .unwrap();

        let limits = RegexLimits::default();
        let err = validate_and_compile_regexes(&config, &limits).unwrap_err();
        assert!(err.contains("invalid regex syntax"));
    }

    #[test]
    fn validate_rejects_regex_exceeding_size_limit() {
        // Create a regex that will exceed compilation size limit
        let large_pattern = format!("({})", "a|b|".repeat(10000));
        let route = make_test_route_with_regex(&large_pattern);
        let config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(default_telemetry())
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(default_listener())
            .add_route(route)
            .build()
            .unwrap();

        let limits = RegexLimits {
            size_limit_bytes: 1024, // Very small limit
            ..Default::default()
        };
        let err = validate_and_compile_regexes(&config, &limits).unwrap_err();
        assert!(err.contains("regex compilation failed") || err.contains("exceeds size limit"));
    }

    #[test]
    fn validate_enforces_per_route_limit() {
        let mut paths = Vec::new();
        for i in 0..15 {
            paths.push(Route {
                matcher: RouteMatcher {
                    path: PathMatch::Prefix {
                        path: Path(format!("/{}", i)),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::Some(vec![HeaderPredicate {
                        name: CompactString::new(format!("x-header-{}", i)),
                        matcher: HeaderMatch::Regex(CompactString::new(format!("pattern{}", i))),
                    }]),
                },
                timeout: pavis_core::Timeout::Disabled,
                retry: pavis_core::RetryPolicy::Disabled,
                request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: pavis_core::Rewrite {
                    path: pavis_core::RewritePath::Disabled,
                    host: pavis_core::RewriteHost::Disabled,
                },
                action: pavis_core::RouteAction::Direct {
                    status: 200,
                    body: "ok".to_string(),
                },
            });
        }

        let route = VirtualHost {
            host: Host("example.com".to_string()),
            paths,
        };

        let config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(default_telemetry())
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(default_listener())
            .add_route(route)
            .build()
            .unwrap();

        let limits = RegexLimits {
            max_regex_per_route: 10,
            ..Default::default()
        };

        let err = validate_and_compile_regexes(&config, &limits).unwrap_err();
        assert!(err.contains("too many regexes"));
        assert!(err.contains("exceeds limit of 10"));
    }

    #[test]
    fn validate_enforces_global_limit() {
        let mut routes = Vec::new();
        for i in 0..15 {
            routes.push(make_test_route_with_regex(&format!("pattern{}", i)));
        }

        let mut builder = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(default_telemetry())
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(default_listener());

        for route in routes {
            builder = builder.add_route(route);
        }

        let config = builder.build().unwrap();

        let limits = RegexLimits {
            max_regex_per_config: 10,
            ..Default::default()
        };

        let err = validate_and_compile_regexes(&config, &limits).unwrap_err();
        assert!(err.contains("too many regexes total"));
        assert!(err.contains("exceeds limit of 10"));
    }

    #[test]
    fn compiled_regex_is_match_respects_input_limit() {
        let pattern = "test";
        let regex = Regex::new(pattern).unwrap();
        let compiled = CompiledRegex {
            pattern: pattern.to_string(),
            compiled: Arc::new(regex),
        };

        // Short input within limit
        assert!(compiled.is_match(b"test", 100));

        // Input exceeds limit
        assert!(!compiled.is_match(b"test", 2));
    }

    #[test]
    fn deduplicates_identical_patterns() {
        let route = VirtualHost {
            host: Host("example.com".to_string()),
            paths: vec![
                Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Prefix {
                            path: Path("/a".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::Some(vec![HeaderPredicate {
                            name: CompactString::new("x-version"),
                            matcher: HeaderMatch::Regex(CompactString::new("^v[0-9]+$")),
                        }]),
                    },
                    timeout: pavis_core::Timeout::Disabled,
                    retry: pavis_core::RetryPolicy::Disabled,
                    request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                    response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: pavis_core::Rewrite {
                        path: pavis_core::RewritePath::Disabled,
                        host: pavis_core::RewriteHost::Disabled,
                    },
                    action: pavis_core::RouteAction::Direct {
                        status: 200,
                        body: "ok".to_string(),
                    },
                },
                Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Prefix {
                            path: Path("/b".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::Some(vec![HeaderPredicate {
                            name: CompactString::new("x-api-version"),
                            matcher: HeaderMatch::Regex(CompactString::new("^v[0-9]+$")),
                        }]),
                    },
                    timeout: pavis_core::Timeout::Disabled,
                    retry: pavis_core::RetryPolicy::Disabled,
                    request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                    response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: pavis_core::Rewrite {
                        path: pavis_core::RewritePath::Disabled,
                        host: pavis_core::RewriteHost::Disabled,
                    },
                    action: pavis_core::RouteAction::Direct {
                        status: 200,
                        body: "ok".to_string(),
                    },
                },
            ],
        };

        let config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(default_telemetry())
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(default_listener())
            .add_route(route)
            .build()
            .unwrap();

        let limits = RegexLimits::default();
        let compiled = validate_and_compile_regexes(&config, &limits).unwrap();

        // Same pattern used twice, should only compile once
        assert_eq!(compiled.len(), 1);
        assert!(compiled.contains_key("^v[0-9]+$"));
    }

    #[test]
    fn count_regexes_in_headers_counts_correctly() {
        let headers = HeaderPredicates::Some(vec![
            HeaderPredicate {
                name: CompactString::new("x-version"),
                matcher: HeaderMatch::Regex(CompactString::new("v.*")),
            },
            HeaderPredicate {
                name: CompactString::new("x-tenant"),
                matcher: HeaderMatch::Exact(CompactString::new("alice")),
            },
            HeaderPredicate {
                name: CompactString::new("x-region"),
                matcher: HeaderMatch::Regex(CompactString::new("us-.*")),
            },
        ]);

        assert_eq!(count_regexes_in_headers(&headers), 2);

        let no_regex_headers = HeaderPredicates::Some(vec![HeaderPredicate {
            name: CompactString::new("x-tenant"),
            matcher: HeaderMatch::Exact(CompactString::new("alice")),
        }]);

        assert_eq!(count_regexes_in_headers(&no_regex_headers), 0);
        assert_eq!(count_regexes_in_headers(&HeaderPredicates::None), 0);
    }
}
