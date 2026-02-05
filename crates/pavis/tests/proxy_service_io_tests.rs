mod common;

use common::*;
use pavis::proxy::service::test_exports::Proxy;
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis::upstream::Manager;
use pavis_core::{
    CircuitBreakerPolicy, Discovery, HeaderPredicates, HeadersPolicy, HttpVersion, LoadBalancer,
    Path, PathMatch, Principal, Rewrite, RewriteHost, RewritePath, Route, RouteAction,
    RouteMatcher, UpstreamBuilder, UpstreamId, UpstreamName, VirtualHost,
};
use pingora::prelude::*;
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn request_filter_rbac_denied() {
    let mut config = base_config();
    config.routes = vec![VirtualHost {
        host: pavis_core::Host("*".to_string()),
        paths: vec![Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: pavis_core::Timeout::Disabled,
            retry: pavis_core::RetryPolicy::Disabled,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            principal: Principal::Authenticated {
                spiffe: pavis_core::SpiffeId("spiffe://example.org/allowed".to_string()),
            },
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Direct {
                status: 200,
                body: "ok".to_string(),
            },
        }],
    }];

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(
            RuntimeState::from_config(&pavis_core::validate_runtime(config).unwrap()).unwrap(),
        )),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);

    // No identity, should be denied
    let handled = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("filter");
    assert!(handled);
    assert!(ctx.rbac_denied);
}

#[tokio::test]
async fn request_filter_redirect() {
    let mut config = base_config();
    config.routes = vec![VirtualHost {
        host: pavis_core::Host("*".to_string()),
        paths: vec![Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/old".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: pavis_core::Timeout::Disabled,
            retry: pavis_core::RetryPolicy::Disabled,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            principal: Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Redirect {
                status: 301,
                location: "https://example.com/new".to_string(),
            },
        }],
    }];

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(
            RuntimeState::from_config(&pavis_core::validate_runtime(config).unwrap()).unwrap(),
        )),
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /old HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);

    let handled = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("filter");
    assert!(handled);

    let mut buf = [0u8; 1024];
    let n = client.read(&mut buf).await.expect("read response");
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(resp.contains("HTTP/1.1 301"));
    assert!(resp.contains("Location: https://example.com/new"));
}

#[tokio::test]
async fn request_filter_direct_response() {
    let mut config = base_config();
    config.routes = vec![VirtualHost {
        host: pavis_core::Host("*".to_string()),
        paths: vec![Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/direct".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: pavis_core::Timeout::Disabled,
            retry: pavis_core::RetryPolicy::Disabled,
            request_headers: Arc::new(HeadersPolicy::Disabled),
            response_headers: Arc::new(HeadersPolicy::Disabled),
            principal: Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Direct {
                status: 200,
                body: "direct ok".to_string(),
            },
        }],
    }];

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(
            RuntimeState::from_config(&pavis_core::validate_runtime(config).unwrap()).unwrap(),
        )),
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /direct HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);

    let handled = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("filter");
    assert!(handled);

    let mut buf = [0u8; 1024];
    let n = client.read(&mut buf).await.expect("read response");
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(resp.contains("HTTP/1.1 200"));
    assert!(resp.contains("direct ok"));
}

#[tokio::test]
async fn upstream_peer_circuit_breaker_rejected() {
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("breaker-test".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .circuit_breaker(CircuitBreakerPolicy::Enabled {
            max_connections: pavis_core::MaxConnections(NonZeroU32::new(1).unwrap()),
            max_pending_requests: pavis_core::MaxPendingRequests(NonZeroU32::new(1).unwrap()),
        })
        .add_endpoint(pavis_core::Endpoint {
            address: pavis_core::EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: pavis_core::Port(NonZeroU16::new(8080).unwrap()),
            },
            weight: pavis_core::Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream");

    let manager = Manager::new(&[upstream_cfg]).expect("manager");
    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(vec![]).unwrap()),
        manager,
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let (mut session1, _client1) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx1 = proxy.new_ctx();
    pin_runtime_state(&mut ctx1, &proxy);
    ctx1.upstream_name = Some(UpstreamName("breaker-test".to_string()));

    // Acquire first permit (takes max_connections)
    let _peer1 = proxy
        .upstream_peer(&mut session1, &mut ctx1)
        .await
        .expect("first peer");

    // We want to saturate max_pending_requests so the next call fails with PendingLimit.
    // Since we can't access the internal semaphores, we use a background task to hold the pending permit.
    let cluster = proxy
        .state
        .load()
        .upstream_manager
        .get("breaker-test")
        .unwrap()
        .clone();

    // This call will take the pending permit and then block awaiting max_connections.
    let cluster_clone = cluster.clone();
    let _pending_task = tokio::spawn(async move { cluster_clone.acquire_breaker_permit().await });

    // Give the task a moment to take the pending permit
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let (mut session3, _client3) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx3 = proxy.new_ctx();
    pin_runtime_state(&mut ctx3, &proxy);
    ctx3.upstream_name = Some(UpstreamName("breaker-test".to_string()));

    // Third request should be rejected immediately because max_connections and max_pending are both 1 and occupied.
    let err = proxy
        .upstream_peer(&mut session3, &mut ctx3)
        .await
        .expect_err("should be rejected");
    assert_eq!(err.etype(), &ErrorType::HTTPStatus(503));
    assert!(err.to_string().contains("circuit breaker rejected request"));
}

#[tokio::test]
async fn upstream_peer_no_endpoints() {
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("no-endpoints".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .build()
        .expect("upstream");

    let manager = Manager::new(&[upstream_cfg]).expect("manager");
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
            RuntimeState::default().config,
            Arc::new(pavis::router::Router::new(vec![]).unwrap()),
            manager,
        ))),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
    ctx.upstream_name = Some(UpstreamName("no-endpoints".to_string()));

    let err = proxy
        .upstream_peer(&mut session, &mut ctx)
        .await
        .expect_err("should fail");
    assert!(err.to_string().contains("Upstream has no endpoints"));
}
