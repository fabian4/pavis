//! Additional comprehensive tests for regex_validator.rs
//!
//! This test file provides extended coverage for regex validation, compilation,
//! matching behavior, deduplication, and all error paths.

use compact_str::CompactString;
use pavis::regex_validator::{CompiledRegex, validate_and_compile_regexes};
use pavis_core::{
    AccessLogPolicy, AdminConfig, HeaderMatch, HeaderPredicate, HeaderPredicates, Host,
    ListenerBuilder, ListenerName, LogLevel, MethodPredicate, Metrics, Path, PathMatch,
    RegexLimits, Route, RouteAction, RouteMatcher, RuntimeConfigBuilder, ServiceName,
    ShutdownPolicy, Telemetry, TlsConfig, TracingPolicy, VirtualHost, WorkerCount,
};
use regex::bytes::Regex;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

fn test_telemetry() -> Telemetry {
    Telemetry {
        level: LogLevel::Info,
        pingora: LogLevel::Warn,
        service_name: ServiceName("test-service".to_string()),
        metrics: Metrics::Disabled,
        access_log: AccessLogPolicy::Disabled,
        tracing: TracingPolicy::Disabled,
    }
}

fn test_listener() -> pavis_core::Listener {
    ListenerBuilder::new()
        .name(ListenerName("test-listener".to_string()))
        .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
        .workers(WorkerCount::Auto)
        .tls(TlsConfig::Disabled)
        .build()
        .unwrap()
}

fn make_route_with_regex(pattern: &str, header_name: &str) -> VirtualHost {
    VirtualHost {
        host: Host("example.com".to_string()),
        paths: vec![Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::Some(vec![HeaderPredicate {
                    name: CompactString::new(header_name),
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
            action: RouteAction::Direct {
                status: 200,
                body: "ok".to_string(),
            },
        }],
    }
}

#[test]
fn test_compiled_regex_is_match_exact() {
    let pattern = "^test$";
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    assert!(compiled.is_match(b"test", 1000));
    assert!(!compiled.is_match(b"testing", 1000));
    assert!(!compiled.is_match(b"atest", 1000));
}

#[test]
fn test_compiled_regex_is_match_input_at_limit() {
    let pattern = "test";
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    // Input length exactly equals max_len
    assert!(compiled.is_match(b"test", 4));
}

#[test]
fn test_compiled_regex_is_match_input_one_over_limit() {
    let pattern = "test";
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    // Input length exceeds max_len by 1
    assert!(!compiled.is_match(b"test", 3));
}

#[test]
fn test_compiled_regex_is_match_empty_input() {
    let pattern = ".*";
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    assert!(compiled.is_match(b"", 100));
}

#[test]
fn test_compiled_regex_is_match_zero_limit() {
    let pattern = "test";
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    // Zero limit should reject all input (including empty)
    assert!(!compiled.is_match(b"test", 0));
    assert!(!compiled.is_match(b"", 0)); // Empty input also rejected with zero limit
}

#[test]
fn test_validate_and_compile_empty_config() {
    let config = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener())
        .build()
        .unwrap();

    let limits = RegexLimits::default();
    let result = validate_and_compile_regexes(&config, &limits).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_validate_and_compile_multiple_different_patterns() {
    let route = VirtualHost {
        host: Host("example.com".to_string()),
        paths: vec![Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/api".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::Some(vec![
                    HeaderPredicate {
                        name: CompactString::new("x-version"),
                        matcher: HeaderMatch::Regex(CompactString::new("^v[0-9]+$")),
                    },
                    HeaderPredicate {
                        name: CompactString::new("x-tenant"),
                        matcher: HeaderMatch::Regex(CompactString::new("^team-.*")),
                    },
                ]),
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
            action: RouteAction::Direct {
                status: 200,
                body: "ok".to_string(),
            },
        }],
    };

    let config = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener())
        .add_route(route)
        .build()
        .unwrap();

    let limits = RegexLimits::default();
    let compiled = validate_and_compile_regexes(&config, &limits).unwrap();

    assert_eq!(compiled.len(), 2);
    assert!(compiled.contains_key("^v[0-9]+$"));
    assert!(compiled.contains_key("^team-.*"));
}

#[test]
fn test_validate_and_compile_routes_without_headers() {
    let route = VirtualHost {
        host: Host("example.com".to_string()),
        paths: vec![Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::None,
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
            action: RouteAction::Direct {
                status: 200,
                body: "ok".to_string(),
            },
        }],
    };

    let config = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener())
        .add_route(route)
        .build()
        .unwrap();

    let limits = RegexLimits::default();
    let result = validate_and_compile_regexes(&config, &limits).unwrap();

    assert_eq!(result.len(), 0);
}

#[test]
fn test_validate_and_compile_mixed_header_matchers() {
    let route = VirtualHost {
        host: Host("example.com".to_string()),
        paths: vec![Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::Some(vec![
                    HeaderPredicate {
                        name: CompactString::new("x-version"),
                        matcher: HeaderMatch::Exact(CompactString::new("v1")),
                    },
                    HeaderPredicate {
                        name: CompactString::new("x-tenant"),
                        matcher: HeaderMatch::Regex(CompactString::new("^team-.*")),
                    },
                    HeaderPredicate {
                        name: CompactString::new("x-region"),
                        matcher: HeaderMatch::Prefix(CompactString::new("us-")),
                    },
                ]),
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
            action: RouteAction::Direct {
                status: 200,
                body: "ok".to_string(),
            },
        }],
    };

    let config = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener())
        .add_route(route)
        .build()
        .unwrap();

    let limits = RegexLimits::default();
    let compiled = validate_and_compile_regexes(&config, &limits).unwrap();

    // Only regex matcher should be compiled
    assert_eq!(compiled.len(), 1);
    assert!(compiled.contains_key("^team-.*"));
}

#[test]
fn test_validate_regex_error_includes_route_path_header_info() {
    let route = make_route_with_regex("[invalid", "x-custom-header");

    let config = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener())
        .add_route(route)
        .build()
        .unwrap();

    let limits = RegexLimits::default();
    let err = validate_and_compile_regexes(&config, &limits).unwrap_err();

    assert!(err.contains("routes[0]"));
    assert!(err.contains("paths[0]"));
    assert!(err.contains("headers[0]"));
    assert!(err.contains("x-custom-header"));
    assert!(err.contains("invalid regex syntax"));
}

#[test]
fn test_validate_per_route_limit_exact_at_limit() {
    let mut paths = Vec::new();
    for i in 0..10 {
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
            action: RouteAction::Direct {
                status: 200,
                body: "ok".to_string(),
            },
        });
    }

    let route = VirtualHost {
        host: Host("example.com".to_string()),
        paths,
    };

    let config = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener())
        .add_route(route)
        .build()
        .unwrap();

    let limits = RegexLimits {
        max_regex_per_route: 10,
        ..Default::default()
    };

    // Should succeed at exactly the limit
    let result = validate_and_compile_regexes(&config, &limits);
    assert!(result.is_ok());
}

#[test]
fn test_validate_global_limit_exact_at_limit() {
    let mut builder = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener());

    for i in 0..10 {
        let route = make_route_with_regex(&format!("pattern{}", i), "x-version");
        builder = builder.add_route(route);
    }

    let config = builder.build().unwrap();

    let limits = RegexLimits {
        max_regex_per_config: 10,
        ..Default::default()
    };

    // Should succeed at exactly the limit
    let result = validate_and_compile_regexes(&config, &limits);
    assert!(result.is_ok());
}

#[test]
fn test_validate_complex_regex_patterns() {
    let patterns = vec![
        "^[a-zA-Z0-9]+$",
        "\\d{3}-\\d{4}",
        "^(GET|POST|PUT)$",
        ".*@example\\.com$",
        "^v[0-9]+\\.[0-9]+\\.[0-9]+$",
    ];

    for pattern in patterns {
        let route = make_route_with_regex(pattern, "x-test");
        let config = RuntimeConfigBuilder::new()
            .telemetry(test_telemetry())
            .shutdown(ShutdownPolicy::Disabled)
            .admin(AdminConfig::Disabled)
            .add_listener(test_listener())
            .add_route(route)
            .build()
            .unwrap();

        let limits = RegexLimits::default();
        let result = validate_and_compile_regexes(&config, &limits);
        assert!(
            result.is_ok(),
            "Pattern '{}' should compile successfully",
            pattern
        );
    }
}

#[test]
fn test_validate_invalid_regex_patterns() {
    let invalid_patterns = vec![
        "[unclosed",
        "(unclosed",
        "(?P<invalid",
        "*invalid",
        "(?P<>empty)",
    ];

    for pattern in invalid_patterns {
        let route = make_route_with_regex(pattern, "x-test");
        let config = RuntimeConfigBuilder::new()
            .telemetry(test_telemetry())
            .shutdown(ShutdownPolicy::Disabled)
            .admin(AdminConfig::Disabled)
            .add_listener(test_listener())
            .add_route(route)
            .build()
            .unwrap();

        let limits = RegexLimits::default();
        let result = validate_and_compile_regexes(&config, &limits);
        assert!(
            result.is_err(),
            "Pattern '{}' should fail validation",
            pattern
        );
    }
}

#[test]
fn test_compiled_regex_debug_format() {
    let pattern = "test.*";
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    let debug_str = format!("{:?}", compiled);
    assert!(debug_str.contains("CompiledRegex"));
}

#[test]
fn test_compiled_regex_clone() {
    let pattern = "test";
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    let cloned = compiled.clone();
    assert_eq!(compiled.pattern, cloned.pattern);
    assert!(cloned.is_match(b"test", 100));
}

#[test]
fn test_validate_deduplication_across_routes() {
    let route1 = make_route_with_regex("^v[0-9]+$", "x-version");
    let route2 = make_route_with_regex("^v[0-9]+$", "x-api-version");

    let config = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener())
        .add_route(route1)
        .add_route(route2)
        .build()
        .unwrap();

    let limits = RegexLimits::default();
    let compiled = validate_and_compile_regexes(&config, &limits).unwrap();

    // Same pattern across routes should only be compiled once
    assert_eq!(compiled.len(), 1);
}

#[test]
fn test_validate_size_limit_error_message() {
    let large_pattern = format!("({})", "a|b|".repeat(5000));
    let route = make_route_with_regex(&large_pattern, "x-test");

    let config = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener())
        .add_route(route)
        .build()
        .unwrap();

    let limits = RegexLimits {
        size_limit_bytes: 1024,
        ..Default::default()
    };

    let err = validate_and_compile_regexes(&config, &limits).unwrap_err();
    assert!(err.contains("regex compilation failed") || err.contains("size limit"));
}

#[test]
fn test_compiled_regex_match_with_large_input() {
    let pattern = "test";
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    let large_input = vec![b't'; 10000];
    // Large input exceeding limit should return false
    assert!(!compiled.is_match(&large_input, 100));
}

#[test]
fn test_compiled_regex_match_escaped_sequences() {
    // Test matching escaped sequences in text data
    let pattern = r"test\\data"; // Matches "test\data"
    let regex = Regex::new(pattern).unwrap();
    let compiled = CompiledRegex {
        pattern: pattern.to_string(),
        compiled: Arc::new(regex),
    };

    // Test matching text with backslash
    assert!(compiled.is_match(b"test\\data", 100));
    assert!(!compiled.is_match(b"testdata", 100));
}

#[test]
fn test_validate_multiple_routes_different_patterns() {
    let routes = vec![
        make_route_with_regex("pattern1", "x-header1"),
        make_route_with_regex("pattern2", "x-header2"),
        make_route_with_regex("pattern3", "x-header3"),
    ];

    let mut builder = RuntimeConfigBuilder::new()
        .telemetry(test_telemetry())
        .shutdown(ShutdownPolicy::Disabled)
        .admin(AdminConfig::Disabled)
        .add_listener(test_listener());

    for route in routes {
        builder = builder.add_route(route);
    }

    let config = builder.build().unwrap();
    let limits = RegexLimits::default();
    let compiled = validate_and_compile_regexes(&config, &limits).unwrap();

    assert_eq!(compiled.len(), 3);
    assert!(compiled.contains_key("pattern1"));
    assert!(compiled.contains_key("pattern2"));
    assert!(compiled.contains_key("pattern3"));
}
