mod common;

use common::*;
use pavis::proxy::service::test_exports::{Proxy, calculate_path_rewrite, route_path};
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis::upstream::Manager;
use pavis_core::{
    Destination, HeaderPredicates, HeadersPolicy, Host, Hostname, Path, PathMatch, RetryPolicy,
    Rewrite, RewriteHost, RewritePath, RouteAction, RouteMatcher, Timeout, UpstreamName,
    VirtualHost, Weight,
};
use pingora::prelude::ProxyHttp;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn request_filter_selects_weighted_destination() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Exact {
                    path: Path("/api".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![
                Destination {
                    upstream: UpstreamName("blue".to_string()),
                    weight: Weight(std::num::NonZeroU16::new(1).unwrap()),
                },
                Destination {
                    upstream: UpstreamName("green".to_string()),
                    weight: Weight(std::num::NonZeroU16::new(2).unwrap()),
                },
            ]),
        }],
    }];
    let manager =
        Manager::new(&[upstream("blue", 1, 8081), upstream("green", 2, 8082)]).expect("manager");
    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(routes).expect("routes")),
        manager,
    );
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let (mut session, _client) =
        session_for_request(b"GET /api HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let should_respond = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("request filter");
    assert!(!should_respond);

    let selected = ctx
        .upstream_name
        .as_ref()
        .map(|v| v.0.as_str())
        .expect("upstream selected");
    assert!(selected == "blue" || selected == "green");
}

#[tokio::test]
async fn request_filter_returns_404_when_no_route_matches() {
    let manager = Manager::new(&[]).expect("manager");
    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
        manager,
    );
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /missing HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let should_respond = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("request filter");
    assert!(should_respond);
    let mut buf = [0u8; 512];
    let read = tokio::time::timeout(std::time::Duration::from_secs(1), client.read(&mut buf))
        .await
        .expect("read timeout")
        .expect("read response");
    let response = String::from_utf8_lossy(&buf[..read]);
    assert!(response.contains(" 404 "), "response was {response:?}");
}

#[tokio::test]
async fn request_filter_applies_rewrite_policy() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/api".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Prefix {
                    from: Path("/api".to_string()),
                    to: Path("/v2".to_string()),
                },
                host: RewriteHost::Literal {
                    host: Hostname("rewrite.example.com".to_string()),
                },
            },
            action: RouteAction::Forward(vec![Destination {
                upstream: UpstreamName("backend".to_string()),
                weight: Weight(std::num::NonZeroU16::new(1).unwrap()),
            }]),
        }],
    }];
    let manager = Manager::new(&[upstream("backend", 1, 8081)]).expect("manager");
    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(routes).expect("routes")),
        manager,
    );
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let (mut session, _client) =
        session_for_request(b"GET /api/widgets?id=1 HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let should_respond = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("request filter");
    assert!(!should_respond);

    assert_eq!(
        ctx.rewritten_uri.as_ref().map(|u: &http::Uri| u.path()),
        Some("/v2/widgets")
    );
    assert_eq!(
        ctx.rewritten_uri
            .as_ref()
            .and_then(|u: &http::Uri| u.query()),
        Some("id=1")
    );
    assert_eq!(
        ctx.rewritten_host.as_ref().map(|v| v.0.as_str()),
        Some("rewrite.example.com")
    );
}

#[tokio::test]
async fn request_filter_skips_selection_when_no_destinations() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Exact {
                    path: Path("/api".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(Vec::new()),
        }],
    }];
    let manager =
        Manager::new(&[upstream("blue", 1, 8081), upstream("green", 2, 8082)]).expect("manager");
    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(routes).expect("routes")),
        manager,
    );
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let (mut session, _client) =
        session_for_request(b"GET /api HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let should_respond = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("request filter");
    assert!(!should_respond);
    assert!(ctx.upstream_name.is_none());
}

#[test]
fn test_calculate_path_rewrite() {
    let route_prefix = pavis_core::Route {
        matcher: RouteMatcher {
            path: PathMatch::Prefix {
                path: Path("/api".to_string()),
            },
            method: pavis_core::MethodPredicate::Any,
            headers: HeaderPredicates::None,
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled.into(),
        response_headers: HeadersPolicy::Disabled.into(),
        principal: pavis_core::Principal::Any,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        action: RouteAction::Forward(vec![]),
    };

    let uri = calculate_path_rewrite(&route_prefix, "/api/foo", Some("q=1")).unwrap();
    assert_eq!(uri.path(), "/v2/foo");
    assert_eq!(uri.query(), Some("q=1"));

    let uri = calculate_path_rewrite(&route_prefix, "/api/foo", None).unwrap();
    assert_eq!(uri.path(), "/v2/foo");
    assert_eq!(uri.query(), None);

    let route_exact = pavis_core::Route {
        matcher: RouteMatcher {
            path: PathMatch::Exact {
                path: Path("/api".to_string()),
            },
            method: pavis_core::MethodPredicate::Any,
            headers: HeaderPredicates::None,
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled.into(),
        response_headers: HeadersPolicy::Disabled.into(),
        principal: pavis_core::Principal::Any,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        action: RouteAction::Forward(vec![]),
    };

    let uri = calculate_path_rewrite(&route_exact, "/api", None).unwrap();
    assert_eq!(uri.path(), "/v2");

    assert!(calculate_path_rewrite(&route_exact, "/api/foo", None).is_none());
}

#[test]
fn test_route_path_helper() {
    let r1 = pavis_core::Route {
        matcher: RouteMatcher {
            path: PathMatch::Prefix {
                path: Path("/p".to_string()),
            },
            method: pavis_core::MethodPredicate::Any,
            headers: HeaderPredicates::None,
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled.into(),
        response_headers: HeadersPolicy::Disabled.into(),
        principal: pavis_core::Principal::Any,
        rewrite: Rewrite {
            path: RewritePath::Disabled,
            host: RewriteHost::Disabled,
        },
        action: RouteAction::Forward(vec![]),
    };
    assert_eq!(route_path(&r1), "/p");
}

#[test]
fn test_calculate_path_rewrite_unmatched_prefix() {
    let route = pavis_core::Route {
        matcher: RouteMatcher {
            path: PathMatch::Prefix {
                path: Path("/api".to_string()),
            },
            method: pavis_core::MethodPredicate::Any,
            headers: HeaderPredicates::None,
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled.into(),
        response_headers: HeadersPolicy::Disabled.into(),
        principal: pavis_core::Principal::Any,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        action: RouteAction::Forward(vec![]),
    };

    assert!(calculate_path_rewrite(&route, "/other", None).is_none());
}

#[tokio::test]
async fn request_filter_handles_redirect_action() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Exact {
                    path: Path("/old".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
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

    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(routes).unwrap()),
        Manager::new(&[]).expect("manager"),
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /old HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let result = proxy.request_filter(&mut session, &mut ctx).await;

    assert!(result.is_ok());
    assert!(result.unwrap());

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("301"));
    assert!(response.contains("Location: https://example.com/new"));
}

#[tokio::test]
async fn request_filter_handles_direct_action() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Exact {
                    path: Path("/health".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Direct {
                status: 200,
                body: "OK".to_string(),
            },
        }],
    }];

    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(routes).unwrap()),
        Manager::new(&[]).expect("manager"),
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /health HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let result = proxy.request_filter(&mut session, &mut ctx).await;

    assert!(result.is_ok());
    assert!(result.unwrap());

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("200"));
    assert!(response.contains("Content-Type: text/plain"));
    assert!(response.contains("OK"));
}

#[tokio::test]
async fn request_filter_redirect_with_different_status_codes() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/temp".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Redirect {
                status: 302,
                location: "https://temporary.com".to_string(),
            },
        }],
    }];

    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(routes).unwrap()),
        Manager::new(&[]).expect("manager"),
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /temp HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let result = proxy.request_filter(&mut session, &mut ctx).await;

    assert!(result.is_ok());
    assert!(result.unwrap());

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("302"));
    assert!(response.contains("Location: https://temporary.com"));
}

#[tokio::test]
async fn request_filter_direct_with_custom_status() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/gone".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Direct {
                status: 404,
                body: "Not Found".to_string(),
            },
        }],
    }];

    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(routes).unwrap()),
        Manager::new(&[]).expect("manager"),
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /gone HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let result = proxy.request_filter(&mut session, &mut ctx).await;

    assert!(result.is_ok());
    assert!(result.unwrap());

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("404"));
    assert!(response.contains("Not Found"));
}

#[test]
fn test_calculate_path_rewrite_preserves_query_string() {
    let route = pavis_core::Route {
        matcher: RouteMatcher {
            path: PathMatch::Prefix {
                path: Path("/api/v1".to_string()),
            },
            method: pavis_core::MethodPredicate::Any,
            headers: HeaderPredicates::None,
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled.into(),
        response_headers: HeadersPolicy::Disabled.into(),
        principal: pavis_core::Principal::Any,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api/v1".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        action: RouteAction::Forward(vec![]),
    };

    let uri = calculate_path_rewrite(
        &route,
        "/api/v1/users",
        Some("id=123&filter=active&sort=name"),
    )
    .unwrap();
    assert_eq!(uri.path(), "/v2/users");
    assert_eq!(uri.query(), Some("id=123&filter=active&sort=name"));
}

#[tokio::test]
async fn request_filter_applies_rewrite_and_preserves_query() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/old-api".to_string()),
                },
                method: pavis_core::MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Prefix {
                    from: Path("/old-api".to_string()),
                    to: Path("/new-api".to_string()),
                },
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![Destination {
                upstream: UpstreamName("backend".to_string()),
                weight: Weight(std::num::NonZeroU16::new(1).unwrap()),
            }]),
        }],
    }];
    let manager = Manager::new(&[upstream("backend", 1, 8081)]).expect("manager");
    let state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(routes).expect("routes")),
        manager,
    );
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(
        b"GET /old-api/resource?filter=active&limit=10 HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;
    let mut ctx = proxy.new_ctx();
    let should_respond = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("request filter");
    assert!(!should_respond);

    assert_eq!(
        ctx.rewritten_uri.as_ref().map(|u: &http::Uri| u.path()),
        Some("/new-api/resource")
    );
    assert_eq!(
        ctx.rewritten_uri
            .as_ref()
            .and_then(|u: &http::Uri| u.query()),
        Some("filter=active&limit=10")
    );
}
