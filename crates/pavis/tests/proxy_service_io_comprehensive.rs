mod common;

use common::*;
use pavis::proxy::service::test_exports::Proxy;
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis::upstream::Manager;
use pavis_core::{RetryPolicy, RetryReason, RetryableStatusCodes, TryTimeout, UpstreamName};
use pingora::prelude::*;
use std::num::NonZeroU16;
use std::sync::Arc;

#[tokio::test]
async fn test_response_filter_retryable_status() {
    let mut config = minimal_config("test");
    // Enable retries for 503
    config.routes[0].paths[0].retry = RetryPolicy::Enabled {
        max_attempts: NonZeroU16::new(3).unwrap(),
        retryable_reasons: vec![RetryReason::StatusCode],
        retryable_status_codes: Some(RetryableStatusCodes { codes: vec![503] }),
        backoff: pavis_core::BackoffStrategy::Fixed { base_ms: 10 },
        retry_non_idempotent: false,
        fail_on_non_replayable_retry: false,
        max_request_body_buffer_bytes: 1024,
        per_try: TryTimeout::Inherit,
    };

    let validated = pavis_core::validate_runtime(config).expect("validation");
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(
            RuntimeState::from_config(&validated).unwrap(),
        )),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);

    // Initialize retry context
    if let RetryPolicy::Enabled { .. } = &validated.routes[0].paths[0].retry {
        ctx.retry_policy = validated.routes[0].paths[0].retry.clone();
        ctx.retry_ctx = Some(pavis::retry::RetryContext::new(
            ctx.retry_policy.clone(),
            10000,
            None,
            "backend".to_string(),
        ));
    }

    let mut response = pingora::http::ResponseHeader::build(503, None).unwrap();

    let result = proxy
        .response_filter(&mut session, &mut response, &mut ctx)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("retryable upstream response"));
    assert!(err.retry());
    assert_eq!(ctx.retry_ctx.as_ref().unwrap().attempt, 2);
}

#[tokio::test]
async fn test_logging_outcome_recording() {
    let config = minimal_config("test");
    let validated = pavis_core::validate_runtime(config).expect("validation");

    let upstream_backend = upstream("backend", 1, 8080);
    let manager = Manager::new(&[upstream_backend]).expect("manager");
    let state = RuntimeState::with_components(
        validated,
        Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
        manager,
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("backend".to_string()));
    ctx.upstream_endpoint = Some(pavis_core::EndpointAddr::Ip {
        address: "127.0.0.1".parse().unwrap(),
        port: pavis_core::Port(NonZeroU16::new(8080).unwrap()),
    });
    pin_runtime_state(&mut ctx, &proxy);
    ctx.start_upstream();

    // Mock a response written
    // pingora doesn't easily let us mock session.response_written() return value
    // but we can call logging and see it doesn't panic
    proxy.logging(&mut session, None, &mut ctx).await;
}

#[tokio::test]
async fn test_upstream_request_filter_with_rewrites() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::default())),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.rewritten_uri = Some("/new-path".parse().unwrap());
    ctx.rewritten_host = Some(pavis_core::Hostname("new-host".to_string()));

    let mut req = pingora::http::RequestHeader::build("GET", b"/", None).unwrap();

    proxy
        .upstream_request_filter(&mut session, &mut req, &mut ctx)
        .await
        .unwrap();

    assert_eq!(req.uri.path(), "/new-path");
    assert_eq!(req.headers.get("Host").unwrap(), "new-host");
}

#[tokio::test]
async fn test_logging_not_matched() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::default())),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    // RoutePattern defaults to NotMatched

    proxy.logging(&mut session, None, &mut ctx).await;
}

#[tokio::test]
async fn test_response_filter_method_not_retriable() {
    let mut config = minimal_config("test");
    // Enable retries for 503 but NOT for POST (default)
    config.routes[0].paths[0].retry = RetryPolicy::Enabled {
        max_attempts: NonZeroU16::new(3).unwrap(),
        retryable_reasons: vec![RetryReason::StatusCode],
        retryable_status_codes: Some(RetryableStatusCodes { codes: vec![503] }),
        backoff: pavis_core::BackoffStrategy::Fixed { base_ms: 10 },
        retry_non_idempotent: false,
        fail_on_non_replayable_retry: false,
        max_request_body_buffer_bytes: 1024,
        per_try: TryTimeout::Inherit,
    };

    let validated = pavis_core::validate_runtime(config).expect("validation");
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(
            RuntimeState::from_config(&validated).unwrap(),
        )),
        telemetry: test_telemetry(),
    };

    // Use POST request
    let (mut session, _client) = session_for_request(b"POST / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);

    ctx.retry_policy = validated.routes[0].paths[0].retry.clone();
    ctx.retry_ctx = Some(pavis::retry::RetryContext::new(
        ctx.retry_policy.clone(),
        10000,
        None,
        "backend".to_string(),
    ));

    let mut response = pingora::http::ResponseHeader::build(503, None).unwrap();

    let result = proxy
        .response_filter(&mut session, &mut response, &mut ctx)
        .await;

    // Should NOT retry because POST is not idempotent and retry_non_idempotent is false
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_response_filter_body_not_replayable() {
    let mut config = minimal_config("test");
    // Enable retries for 503 and set fail_on_non_replayable_retry to true
    config.routes[0].paths[0].retry = RetryPolicy::Enabled {
        max_attempts: NonZeroU16::new(3).unwrap(),
        retryable_reasons: vec![RetryReason::StatusCode],
        retryable_status_codes: Some(RetryableStatusCodes { codes: vec![503] }),
        backoff: pavis_core::BackoffStrategy::Fixed { base_ms: 10 },
        retry_non_idempotent: true, // Allow POST retry
        fail_on_non_replayable_retry: true,
        max_request_body_buffer_bytes: 1024,
        per_try: TryTimeout::Inherit,
    };

    let validated = pavis_core::validate_runtime(config).expect("validation");
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(
            RuntimeState::from_config(&validated).unwrap(),
        )),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(b"POST / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);

    ctx.retry_policy = validated.routes[0].paths[0].retry.clone();
    ctx.retry_ctx = Some(pavis::retry::RetryContext::new(
        ctx.retry_policy.clone(),
        10000,
        None,
        "backend".to_string(),
    ));

    // Mark body as NOT replayable (exceeded buffer)
    ctx.buffered_body = Some(pavis::retry::BufferedBody::new(
        vec![1, 2, 3],
        1024,
        None,
        "backend",
        true, // force_streaming = true makes it not replayable
        None,
    ));

    let mut response = pingora::http::ResponseHeader::build(503, None).unwrap();

    let result = proxy
        .response_filter(&mut session, &mut response, &mut ctx)
        .await;

    // Should return 500 error because body is not replayable and policy says fail
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.etype(), &pingora::ErrorType::HTTPStatus(500));
}

#[tokio::test]
async fn test_upstream_request_filter_with_tracing() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::default())),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();

    // Initialize tracing
    let span = tracing::Span::none();
    ctx.telemetry
        .replace_span(pavis::proxy::context::TracingSpan::Active(span));

    let mut upstream_req = pingora::http::RequestHeader::build("GET", b"/", None).unwrap();

    proxy
        .upstream_request_filter(&mut session, &mut upstream_req, &mut ctx)
        .await
        .unwrap();

    // Just verifying it doesn't panic when a span is present.
}
