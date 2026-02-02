mod common;

use common::*;
use opentelemetry::propagation::Injector;
use pavis::proxy::context::{RequestTelemetry, RouterContext};
use pavis::proxy::service::test_exports::{HeaderInjector, Proxy, apply_route_headers};
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis::upstream::Manager;
use pavis_core::{
    Destination, Duration, HeaderName, HeaderPredicates, HeaderValue, Headers, HeadersPolicy, Path,
    PathMatch, RetryPolicy, Rewrite, RewriteHost, RewritePath, RouteAction, RouteMatcher, Timeout,
    UpstreamName, Weight,
};
use pingora::http::ResponseHeader;
use pingora::prelude::{ProxyHttp, RequestHeader};
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;
use std::time::Instant;

#[test]
fn apply_route_headers_populates_router_context() {
    let route = pavis_core::Route {
        matcher: RouteMatcher {
            path: PathMatch::Exact {
                path: Path("/".to_string()),
            },
            method: pavis_core::MethodPredicate::Any,
            headers: HeaderPredicates::None,
        },
        timeout: Timeout::Enabled(Duration(NonZeroU32::new(500).unwrap())),
        retry: RetryPolicy::Enabled {
            max_attempts: NonZeroU16::new(2).unwrap(),
            per_try: pavis_core::TryTimeout::Inherit,
            retryable_reasons: vec![pavis_core::RetryReason::StatusCode],
            retryable_status_codes: Some(pavis_core::RetryableStatusCodes {
                codes: vec![502, 503, 504],
            }),
            backoff: pavis_core::BackoffStrategy::Exponential {
                base_ms: 100,
                max_ms: 5000,
            },
            retry_non_idempotent: false,
            fail_on_non_replayable_retry: false,
            max_request_body_buffer_bytes: 1_048_576,
        },
        request_headers: HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![(
                    HeaderName("x-req".to_string()),
                    HeaderValue("1".to_string()),
                )],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: vec![HeaderName("x-remove".to_string())],
            },
        }
        .into(),
        response_headers: HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![(
                    HeaderName("x-resp".to_string()),
                    HeaderValue("ok".to_string()),
                )],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        }
        .into(),
        principal: pavis_core::Principal::Any,
        rewrite: Rewrite {
            path: RewritePath::Disabled,
            host: RewriteHost::Disabled,
        },
        action: RouteAction::Forward(vec![Destination {
            upstream: UpstreamName("backend".to_string()),
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }]),
    };
    let mut ctx = RouterContext {
        telemetry: RequestTelemetry::new("req-123".parse().unwrap()),
        upstream_name: None,
        upstream_endpoint: None,
        request_headers: Arc::new(HeadersPolicy::Disabled),
        response_headers: Arc::new(HeadersPolicy::Disabled),
        sni_override: None,
        start_time: Instant::now(),
        client_identity: None,
        rbac_denied: false,
        route_timeout: Timeout::Disabled,
        retry_policy: RetryPolicy::Disabled,
        retry_attempts: 0,
        upstream_timing: pavis::proxy::context::UpstreamTiming::NotStarted,
        route_pattern: pavis::proxy::context::RoutePattern::NotMatched,
        pool_permit: None,
        circuit_breaker_permit: None,
        runtime_state: None,
        retry_ctx: None,
        buffered_body: None,
        rewritten_uri: None,
        rewritten_host: None,
    };

    apply_route_headers(&mut ctx, &route);

    assert!(matches!(
        *ctx.request_headers,
        HeadersPolicy::Enabled { .. }
    ));
    assert!(matches!(
        *ctx.response_headers,
        HeadersPolicy::Enabled { .. }
    ));
    assert_eq!(ctx.route_timeout, route.timeout);
    assert!(matches!(ctx.retry_policy, RetryPolicy::Enabled { .. }));
}

#[tokio::test]
async fn upstream_response_filter_applies_headers() {
    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
        Manager::new(&[]).expect("manager"),
    );
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let mut ctx = proxy.new_ctx();
    ctx.response_headers = HeadersPolicy::Enabled {
        rules: Headers {
            set_headers: vec![(
                HeaderName("x-added".to_string()),
                HeaderValue("ok".to_string()),
            )],
            append_headers: Vec::new(),
            add_headers: Vec::new(),
            remove_headers: vec![HeaderName("x-drop".to_string())],
        },
    }
    .into();

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut resp = ResponseHeader::build(200, None).expect("resp");
    resp.insert_header("x-drop", "gone").expect("header");

    proxy
        .upstream_response_filter(&mut session, &mut resp, &mut ctx)
        .await
        .expect("filter");
    assert!(resp.headers.get("x-drop").is_none());
    assert_eq!(resp.headers.get("x-added").unwrap().to_str().unwrap(), "ok");
}

#[tokio::test]
async fn test_upstream_request_filter() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
            RuntimeState::default().config,
            Arc::new(pavis::router::Router::new(vec![]).unwrap()),
            Manager::new(&[]).expect("manager"),
        ))),
        telemetry: test_telemetry(),
    };
    let mut ctx = proxy.new_ctx();
    ctx.request_headers = HeadersPolicy::Enabled {
        rules: Headers {
            set_headers: vec![(
                HeaderName("x-forwarded-for".to_string()),
                HeaderValue("1.2.3.4".to_string()),
            )],
            append_headers: Vec::new(),
            add_headers: Vec::new(),
            remove_headers: Vec::new(),
        },
    }
    .into();

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut req = RequestHeader::build("GET", b"/", None).unwrap();
    proxy
        .upstream_request_filter(&mut session, &mut req, &mut ctx)
        .await
        .unwrap();

    assert_eq!(
        req.headers
            .get("x-forwarded-for")
            .unwrap()
            .to_str()
            .unwrap(),
        "1.2.3.4"
    );
}

#[tokio::test]
async fn test_request_filter_direct_response_with_headers() {
    let routes = vec![pavis_core::VirtualHost {
        host: pavis_core::Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Exact {
                    path: Path("/direct".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Enabled {
                rules: Headers {
                    set_headers: vec![(
                        HeaderName("x-direct".to_string()),
                        HeaderValue("true".to_string()),
                    )],
                    append_headers: Vec::new(),
                    add_headers: Vec::new(),
                    remove_headers: Vec::new(),
                },
            }
            .into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Direct {
                status: 200,
                body: "Direct".to_string(),
            },
        }],
    }];

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
            RuntimeState::default().config,
            Arc::new(pavis::router::Router::new(routes).unwrap()),
            Manager::new(&[]).expect("manager"),
        ))),
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) = session_for_request(b"GET /direct HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    proxy.request_filter(&mut session, &mut ctx).await.unwrap();

    let mut buf = vec![0u8; 1024];
    use tokio::io::AsyncReadExt;
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("x-direct: true"));
    assert!(response.contains("Direct"));
}

#[test]
fn header_injector_sets_headers() {
    let mut request = RequestHeader::build("GET", b"/", None).unwrap();
    {
        let mut injector = HeaderInjector(&mut request);
        injector.set("x-test", "value".to_string());
        injector.set("x-another", "123".to_string());
    }

    assert_eq!(request.headers.get("x-test").unwrap(), "value");
    assert_eq!(request.headers.get("x-another").unwrap(), "123");
}
