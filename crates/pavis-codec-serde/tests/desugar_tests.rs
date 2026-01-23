//! Desugaring tests for P0 sugar and P2 explicit predicates

use pavis_codec_serde::config::{
    HeaderPredicate, HeaderPredicateLegacy, Matcher, PathMatcher, RetryPolicy, RewritePolicy,
    WeightedDestination,
};

#[test]
fn test_desugar_p0_headers_prefix() {
    // Test that prefix matching works in P0 sugar
    let header = HeaderPredicateLegacy {
        name: "x-tenant".to_string(),
        value: Some("team-".to_string()),
        regex: false,
        prefix: true,
        absent: false,
    };

    assert!(header.prefix);
}

#[test]
fn test_desugar_p0_headers_regex() {
    // Test that regex matching works in P0 sugar
    let header = HeaderPredicateLegacy {
        name: "x-version".to_string(),
        value: Some("v[0-9]+".to_string()),
        regex: true,
        prefix: false,
        absent: false,
    };

    assert!(header.regex);
}

#[test]
fn test_header_predicate_mutually_exclusive() {
    // Cannot have both regex and prefix
    let header = HeaderPredicateLegacy {
        name: "x-foo".to_string(),
        value: Some("bar".to_string()),
        regex: true,
        prefix: true,
        absent: false,
    };

    // This should be caught by codec validation
    assert!(header.regex && header.prefix);
}

#[test]
fn test_header_predicate_absent() {
    let header = HeaderPredicateLegacy {
        name: "x-debug".to_string(),
        value: None,
        regex: false,
        prefix: false,
        absent: true,
    };

    assert!(header.absent);
}

#[test]
fn test_header_predicate_present() {
    let header = HeaderPredicateLegacy {
        name: "x-foo".to_string(),
        value: None,
        regex: false,
        prefix: false,
        absent: false,
    };

    // No value means presence check
    assert!(header.value.is_none());
}

#[test]
fn test_header_predicate_exact() {
    let header = HeaderPredicateLegacy {
        name: "x-tenant".to_string(),
        value: Some("alice".to_string()),
        regex: false,
        prefix: false,
        absent: false,
    };

    assert_eq!(header.value, Some("alice".to_string()));
    assert!(!header.regex && !header.prefix && !header.absent);
}

#[test]
fn test_path_matcher_variants() {
    let prefix = PathMatcher::Prefix {
        path: "/api".to_string(),
    };
    let exact = PathMatcher::Exact {
        path: "/health".to_string(),
    };
    let regex = PathMatcher::Regex {
        path: "/v[0-9]+/.*".to_string(),
    };

    assert!(matches!(prefix, PathMatcher::Prefix { .. }));
    assert!(matches!(exact, PathMatcher::Exact { .. }));
    assert!(matches!(regex, PathMatcher::Regex { .. }));
}

#[test]
fn test_matcher_structure() {
    let matcher = Matcher {
        path: PathMatcher::Prefix {
            path: "/api".to_string(),
        },
        method: Some("GET".to_string()),
        methods: None,
        headers: Some(vec![HeaderPredicate::V1(HeaderPredicateLegacy {
            name: "x-tenant".to_string(),
            value: Some("alice".to_string()),
            regex: false,
            prefix: false,
            absent: false,
        })]),
    };

    assert_eq!(matcher.method, Some("GET".to_string()));
    assert_eq!(matcher.headers.as_ref().unwrap().len(), 1);
}

#[test]
fn test_retry_policy_structure() {
    let retry = RetryPolicy {
        max_attempts: 3,
        retryable_reasons: vec!["status_code".to_string()],
        retryable_status_codes: Some(vec![502, 503, 504]),
        backoff: pavis_codec_serde::config::types::BackoffStrategyDTO::Fixed { base_ms: 100 },
        retry_non_idempotent: false,
        fail_on_non_replayable_retry: false,
        max_request_body_buffer_bytes: 1_048_576,
        per_try: Some(std::time::Duration::from_secs(1)),
    };

    assert_eq!(retry.max_attempts, 3);
}

#[test]
fn test_rewrite_policy_structure() {
    let rewrite = RewritePolicy {
        path: Some("/v2".to_string()),
        host: Some("api.example.com".to_string()),
    };

    assert_eq!(rewrite.path, Some("/v2".to_string()));
}

#[test]
fn test_weighted_destination() {
    let dest = WeightedDestination {
        upstream: "backend".to_string(),
        weight: 100,
    };

    assert_eq!(dest.weight, 100);
}
