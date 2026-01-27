mod common;

use common::*;
use pavis::proxy::service::test_exports::{Proxy, is_authorized};
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis::upstream::Manager;
use pavis_core::{
    Destination, HeaderPredicates, Host, Path, PathMatch, Principal, RetryPolicy, Rewrite,
    RewriteHost, RewritePath, RouteAction, RouteMatcher, Timeout, UpstreamName, VirtualHost,
    Weight,
};
use pingora::prelude::ProxyHttp;
use std::sync::Arc;

#[tokio::test]
async fn request_filter_denies_when_principal_not_any() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Exact {
                    path: Path("/secure".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: pavis_core::HeadersPolicy::Disabled.into(),
            response_headers: pavis_core::HeadersPolicy::Disabled.into(),
            principal: Principal::Authenticated {
                spiffe: "spiffe://cluster.local/ns/default/sa/admin".to_string(),
            },
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![Destination {
                upstream: UpstreamName("backend".to_string()),
                weight: Weight(std::num::NonZeroU16::new(1).unwrap()),
            }]),
        }],
    }];

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(pavis::router::Router::new(routes).unwrap()),
            upstream_manager: Manager::new(&[]).expect("manager"),
            config_version: None,
        })),
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /secure HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let should_respond = proxy.request_filter(&mut session, &mut ctx).await.unwrap();

    assert!(should_respond);
    assert!(ctx.rbac_denied);

    let mut buf = vec![0u8; 1024];
    use tokio::io::AsyncReadExt;
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);
    assert!(response.contains("403"));
}

#[tokio::test]
async fn request_filter_allows_with_matching_identity() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Exact {
                    path: Path("/secure".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: pavis_core::HeadersPolicy::Disabled.into(),
            response_headers: pavis_core::HeadersPolicy::Disabled.into(),
            principal: Principal::Authenticated {
                spiffe: "spiffe://cluster.local/ns/default/sa/admin".to_string(),
            },
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![Destination {
                upstream: UpstreamName("backend".to_string()),
                weight: Weight(std::num::NonZeroU16::new(1).unwrap()),
            }]),
        }],
    }];

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(pavis::router::Router::new(routes).unwrap()),
            upstream_manager: Manager::new(&[upstream("backend", 1, 8080)]).expect("manager"),
            config_version: None,
        })),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) =
        session_for_request(b"GET /secure HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.client_identity = Some("spiffe://cluster.local/ns/default/sa/admin".to_string());

    let should_respond = proxy.request_filter(&mut session, &mut ctx).await.unwrap();
    assert!(!should_respond);
    assert!(!ctx.rbac_denied);
    assert_eq!(ctx.upstream_name.as_ref().unwrap().0, "backend");
}

#[test]
fn test_is_authorized_principal_variants() {
    let any = Principal::Any;
    let auth = Principal::Authenticated {
        spiffe: "admin".to_string(),
    };
    let prefix = Principal::Prefix {
        prefix: "spiffe://".to_string(),
    };

    assert!(is_authorized(&any, None));
    assert!(is_authorized(&any, Some("spiffe://foo")));
    assert!(is_authorized(&auth, Some("admin")));
    assert!(!is_authorized(&auth, Some("user")));
    assert!(!is_authorized(&auth, None));

    assert!(is_authorized(&prefix, Some("spiffe://abc")));
    assert!(!is_authorized(&prefix, Some("https://foo")));
    assert!(!is_authorized(&prefix, None));
}

#[test]
fn extract_client_identity_returns_none_for_non_tls_session() {
    let (_client, server) = tokio::io::duplex(64);
    let session = Session::new_h1(Box::new(server));
    assert!(pavis::proxy::service::test_exports::extract_client_identity(&session).is_none());
}
