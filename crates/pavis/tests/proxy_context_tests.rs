//! Additional comprehensive tests for proxy/context.rs
//!
//! This test file provides extended coverage for RequestId, RequestTelemetry,
//! RouterContext, phase transitions, and all helper types.

use http::Uri;
use pavis::proxy::context::{
    RequestId, RequestIdParseError, RequestTelemetry, RoutePattern, RouterContext, TracingSpan,
    UpstreamTiming,
};
use pavis_core::{
    EndpointAddr, HeadersPolicy, Hostname, Port, RetryPolicy, SpiffeId, Timeout, UpstreamName,
};
use std::num::NonZeroU16;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn test_request_id_from_parts_basic() {
    let id = RequestId::from_parts(12345, 67890);
    let s = id.as_str();

    assert!(s.starts_with("req-"));
    assert!(s.contains("12345"));
    assert!(s.contains("67890"));
}

#[test]
fn test_request_id_from_parts_zero_values() {
    let id = RequestId::from_parts(0, 0);
    let s = id.as_str();

    assert_eq!(s, "req-0-0");
}

#[test]
fn test_request_id_from_parts_large_values() {
    // Test with large but reasonable values that fit within 48 byte limit
    let id = RequestId::from_parts(999_999_999_999, 999_999_999);
    let s = id.as_str();

    assert!(s.starts_with("req-"));
    assert!(s.len() <= 48); // Within max length
    assert!(s.contains("999999999999"));
    assert!(s.contains("999999999"));
}

#[test]
fn test_request_id_as_str_returns_valid_utf8() {
    let id = RequestId::from_parts(999, 888);
    let s = id.as_str();

    assert!(std::str::from_utf8(s.as_bytes()).is_ok());
}

#[test]
fn test_request_id_display() {
    let id = RequestId::from_parts(111, 222);
    let displayed = format!("{}", id);

    assert_eq!(displayed, id.as_str());
    assert!(displayed.starts_with("req-"));
}

#[test]
fn test_request_id_from_str_valid() {
    let result = RequestId::from_str("req-12345-67890");

    assert!(result.is_ok());
    let id = result.unwrap();
    assert_eq!(id.as_str(), "req-12345-67890");
}

#[test]
fn test_request_id_from_str_short() {
    let result = RequestId::from_str("a");

    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_str(), "a");
}

#[test]
fn test_request_id_from_str_max_length() {
    let s = "a".repeat(48);
    let result = RequestId::from_str(&s);

    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_str(), s);
}

#[test]
fn test_request_id_from_str_too_long() {
    let s = "a".repeat(49);
    let result = RequestId::from_str(&s);

    assert!(result.is_err());
}

#[test]
fn test_request_id_from_str_empty() {
    let result = RequestId::from_str("");

    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_str(), "");
}

#[test]
fn test_request_id_parse_error_display() {
    let err = RequestIdParseError;
    let msg = format!("{}", err);

    assert_eq!(msg, "request id is too long");
}

#[test]
fn test_request_id_serialize() {
    let id = RequestId::from_parts(100, 200);
    let json = serde_json::to_string(&id).unwrap();

    assert!(json.contains("req-"));
    assert!(json.contains("100"));
    assert!(json.contains("200"));
}

#[test]
fn test_request_id_eq_same_values() {
    let id1 = RequestId::from_str("req-123").unwrap();
    let id2 = RequestId::from_str("req-123").unwrap();

    assert_eq!(id1, id2);
}

#[test]
fn test_request_id_eq_different_values() {
    let id1 = RequestId::from_str("req-123").unwrap();
    let id2 = RequestId::from_str("req-456").unwrap();

    assert_ne!(id1, id2);
}

#[test]
fn test_request_id_hash() {
    use std::collections::HashSet;

    let id1 = RequestId::from_str("req-1").unwrap();
    let id2 = RequestId::from_str("req-2").unwrap();
    let id3 = RequestId::from_str("req-1").unwrap();

    let mut set = HashSet::new();
    set.insert(id1);
    set.insert(id2);
    set.insert(id3);

    assert_eq!(set.len(), 2); // id1 and id3 are equal
}

#[test]
fn test_request_id_debug() {
    let id = RequestId::from_str("req-debug").unwrap();
    let debug = format!("{:?}", id);

    assert!(debug.contains("RequestId"));
}

#[test]
fn test_request_id_copy_clone() {
    let id1 = RequestId::from_str("req-copy").unwrap();
    let id2 = id1; // Copy
    let id3 = id1; // Copy (RequestId is Copy, no need for clone)

    assert_eq!(id1, id2);
    assert_eq!(id1, id3);
}

#[test]
fn test_request_telemetry_new() {
    let id = RequestId::from_str("req-tel").unwrap();
    let tel = RequestTelemetry::new(id);

    assert_eq!(tel.request_id(), id);
    assert!(matches!(tel.span(), TracingSpan::Disabled));
}

#[test]
fn test_request_telemetry_request_id() {
    let id = RequestId::from_str("req-123").unwrap();
    let tel = RequestTelemetry::new(id);

    assert_eq!(tel.request_id().as_str(), "req-123");
}

#[test]
fn test_request_telemetry_set_request_id() {
    let id1 = RequestId::from_str("req-1").unwrap();
    let id2 = RequestId::from_str("req-2").unwrap();

    let mut tel = RequestTelemetry::new(id1);
    tel.set_request_id(id2);

    assert_eq!(tel.request_id(), id2);
}

#[test]
fn test_request_telemetry_span() {
    let id = RequestId::from_str("req-span").unwrap();
    let tel = RequestTelemetry::new(id);

    assert!(matches!(tel.span(), TracingSpan::Disabled));
}

#[test]
fn test_request_telemetry_span_mut() {
    let id = RequestId::from_str("req-mut").unwrap();
    let mut tel = RequestTelemetry::new(id);

    let span = tel.span_mut();
    assert!(matches!(span, TracingSpan::Disabled));
}

#[test]
fn test_request_telemetry_replace_span() {
    let id = RequestId::from_str("req-replace").unwrap();
    let mut tel = RequestTelemetry::new(id);

    tel.replace_span(TracingSpan::Disabled);
    assert!(matches!(tel.span(), TracingSpan::Disabled));
}

#[test]
fn test_tracing_span_debug() {
    let span = TracingSpan::Disabled;
    let debug = format!("{:?}", span);

    assert!(debug.contains("Disabled"));
}

#[test]
fn test_upstream_timing_not_started_elapsed() {
    let timing = UpstreamTiming::NotStarted;

    assert_eq!(timing.elapsed(), None);
}

#[test]
fn test_upstream_timing_started_elapsed() {
    let start = Instant::now();
    std::thread::sleep(Duration::from_millis(5));
    let timing = UpstreamTiming::Started(start);

    let elapsed = timing.elapsed();
    assert!(elapsed.is_some());
    assert!(elapsed.unwrap() >= Duration::from_millis(5));
}

#[test]
fn test_upstream_timing_clone() {
    let timing1 = UpstreamTiming::NotStarted;
    let timing2 = timing1.clone();

    assert!(matches!(timing2, UpstreamTiming::NotStarted));
}

#[test]
fn test_upstream_timing_debug() {
    let timing = UpstreamTiming::NotStarted;
    let debug = format!("{:?}", timing);

    assert!(debug.contains("NotStarted"));
}

#[test]
fn test_route_pattern_not_matched_as_label() {
    let pattern = RoutePattern::NotMatched;

    assert_eq!(pattern.as_label(), "-");
}

#[test]
fn test_route_pattern_matched_as_label() {
    let pattern = RoutePattern::Matched {
        pattern: Arc::from("/api/v1/users"),
    };

    assert_eq!(pattern.as_label(), "/api/v1/users");
}

#[test]
fn test_route_pattern_not_matched_as_label_opt() {
    let pattern = RoutePattern::NotMatched;

    assert_eq!(pattern.as_label_opt(), None);
}

#[test]
fn test_route_pattern_matched_as_label_opt() {
    let pattern = RoutePattern::Matched {
        pattern: Arc::from("/health"),
    };

    assert_eq!(pattern.as_label_opt(), Some("/health"));
}

#[test]
fn test_route_pattern_clone() {
    let pattern1 = RoutePattern::Matched {
        pattern: Arc::from("/test"),
    };
    let pattern2 = pattern1.clone();

    assert_eq!(pattern1.as_label(), pattern2.as_label());
}

#[test]
fn test_route_pattern_debug() {
    let pattern = RoutePattern::NotMatched;
    let debug = format!("{:?}", pattern);

    assert!(debug.contains("NotMatched"));
}

#[test]
fn test_router_context_request_duration() {
    let ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-dur").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    std::thread::sleep(Duration::from_millis(2));
    let duration = ctx.request_duration();

    assert!(duration >= Duration::from_millis(2));
}

#[test]
fn test_router_context_upstream_latency_not_started() {
    let ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-lat").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    assert_eq!(ctx.upstream_latency(), None);
}

#[test]
fn test_router_context_upstream_latency_started() {
    let ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-lat2").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::Started(Instant::now()),
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    assert!(ctx.upstream_latency().is_some());
}

#[test]
fn test_router_context_request_id() {
    let id = RequestId::from_str("req-ctx").unwrap();
    let ctx = RouterContext {
        telemetry: RequestTelemetry::new(id),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    assert_eq!(ctx.request_id(), id);
}

#[test]
fn test_router_context_set_request_id() {
    let id1 = RequestId::from_str("req-1").unwrap();
    let id2 = RequestId::from_str("req-2").unwrap();

    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new(id1),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    ctx.set_request_id(id2);
    assert_eq!(ctx.request_id(), id2);
}

#[test]
fn test_router_context_span() {
    let ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-span").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    assert!(matches!(ctx.span(), TracingSpan::Disabled));
}

#[test]
fn test_router_context_start_upstream() {
    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-up").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    ctx.start_upstream();
    assert!(matches!(ctx.upstream_timing, UpstreamTiming::Started(_)));
}

#[test]
fn test_router_context_upstream_label_none() {
    let ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-label").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    assert_eq!(ctx.upstream_label(), "-");
}

#[test]
fn test_router_context_upstream_label_some() {
    let ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-label2").unwrap()),
        upstream_name: Some(UpstreamName("my-backend".to_string())),
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    assert_eq!(ctx.upstream_label(), "my-backend");
}

#[test]
fn test_route_match_mark_rbac_denied() {
    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-rbac").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    {
        let mut route_match = ctx.routing_phase().record_route(Arc::from("/test"));
        route_match.mark_rbac_denied();
    }

    assert!(ctx.rbac_denied);
}

#[test]
fn test_route_match_set_rewritten_uri() {
    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-uri").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    {
        let mut route_match = ctx.routing_phase().record_route(Arc::from("/test"));
        let uri = Uri::from_static("http://example.com/new");
        route_match.set_rewritten_uri(uri);
    }

    assert!(ctx.rewritten_uri.is_some());
}

#[test]
fn test_route_match_set_rewritten_host() {
    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-host").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    {
        let mut route_match = ctx.routing_phase().record_route(Arc::from("/test"));
        route_match.set_rewritten_host(Hostname("newhost.com".to_string()));
    }

    assert!(ctx.rewritten_host.is_some());
    assert_eq!(ctx.rewritten_host.unwrap().0, "newhost.com");
}

#[test]
fn test_route_match_client_identity_none() {
    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-id").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    let route_match = ctx.routing_phase().record_route(Arc::from("/test"));
    assert_eq!(route_match.client_identity(), None);
}

#[test]
fn test_route_match_client_identity_some() {
    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-id2").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: Some(SpiffeId("spiffe://trust.org/service".to_string())),
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    let route_match = ctx.routing_phase().record_route(Arc::from("/test"));
    assert!(route_match.client_identity().is_some());
}

#[test]
fn test_upstream_attempt_set_endpoint() {
    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new(RequestId::from_str("req-ep").unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        upstream_timing: UpstreamTiming::NotStarted,
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        route_pattern: RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    {
        let route_match = ctx.routing_phase().record_route(Arc::from("/test"));
        let mut attempt = route_match.into_upstream_attempt();
        attempt.set_endpoint(EndpointAddr::Dns {
            host: Hostname("backend.example.com".to_string()),
            port: Port(NonZeroU16::new(8080).unwrap()),
        });
    }

    assert!(ctx.upstream_endpoint.is_some());
}

#[test]
fn test_request_id_push_u128_single_digit() {
    let id = RequestId::from_parts(5, 0);
    assert!(id.as_str().contains("5"));
}

#[test]
fn test_request_id_push_u128_multiple_digits() {
    let id = RequestId::from_parts(123456789, 0);
    assert!(id.as_str().contains("123456789"));
}
