use super::{Proxy, apply_route_headers};
use crate::proxy::context::RouterContext;
use crate::router::Router;
use crate::telemetry::Telemetry;
use crate::upstream::Manager;
use pavis_core::{
    AccessLogConfig, ConnectionPoolConfig, Endpoint, HeaderOperations, HttpVersion, LoadBalancer,
    MatchType, Route, TelemetryConfig, Upstream, VirtualHost, WeightedDestination,
};
use pingora::proxy::{ProxyHttp, Session};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn apply_route_headers_populates_router_context() {
    let route = Route {
        match_type: MatchType::Exact,
        path: "/".to_string(),
        timeout_ms: None,
        retry_policy: None,
        request_headers: Some(HeaderOperations {
            add: vec![("x-req".to_string(), "1".to_string())],
            remove: vec!["x-remove".to_string()],
        }),
        response_headers: Some(HeaderOperations {
            add: vec![("x-resp".to_string(), "ok".to_string())],
            remove: vec![],
        }),
        destinations: vec![WeightedDestination {
            upstream: "backend".to_string(),
            weight: 1,
        }],
    };
    let mut ctx = RouterContext {
        upstream_name: None,
        request_headers: None,
        response_headers: None,
        start_time: std::time::Instant::now(),
    };

    apply_route_headers(&mut ctx, &route);

    assert!(ctx.request_headers.is_some());
    assert!(ctx.response_headers.is_some());
}

fn test_telemetry() -> Arc<Telemetry> {
    let (telemetry, _worker) = Telemetry::new(&TelemetryConfig {
        level: None,
        pingora: None,
        service_name: None,
        prometheus_addr: None,
        access_log: AccessLogConfig::Disabled,
        tracing: None,
    });
    Arc::new(telemetry)
}

#[test]
fn new_ctx_defaults_are_empty() {
    let router = Arc::new(Router::new(vec![]).expect("empty routes"));
    let manager = Manager::new(&[]);
    let proxy = Proxy {
        router,
        upstream_manager: manager,
        telemetry: test_telemetry(),
    };

    let before = Instant::now();
    let ctx = proxy.new_ctx();
    assert!(ctx.upstream_name.is_none());
    assert!(ctx.request_headers.is_none());
    assert!(ctx.response_headers.is_none());
    assert!(ctx.start_time >= before);
}

async fn session_for_request(request: &[u8]) -> (Session, tokio::io::DuplexStream) {
    let (mut client, server) = tokio::io::duplex(1024);
    client.write_all(request).await.expect("write request");
    let mut session = Session::new_h1(Box::new(server));
    session.read_request().await.expect("read request");
    (session, client)
}

fn upstream(name: &str, port: u16) -> Upstream {
    Upstream {
        name: name.to_string(),
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![Endpoint {
            ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
            weight: 1,
        }],
    }
}

#[tokio::test]
async fn request_filter_selects_weighted_destination() {
    let routes = vec![VirtualHost {
        host: "*".to_string(),
        paths: vec![Route {
            match_type: MatchType::Exact,
            path: "/api".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            destinations: vec![
                WeightedDestination {
                    upstream: "blue".to_string(),
                    weight: 1,
                },
                WeightedDestination {
                    upstream: "green".to_string(),
                    weight: 2,
                },
            ],
        }],
    }];
    let router = Arc::new(Router::new(routes).expect("routes"));
    let manager = Manager::new(&[upstream("blue", 8081), upstream("green", 8082)]);
    let proxy = Proxy {
        router,
        upstream_manager: manager,
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

    let expected: HashSet<&str> = ["blue", "green"].into_iter().collect();
    let selected = ctx.upstream_name.as_deref().expect("upstream selected");
    assert!(expected.contains(selected));
}

#[tokio::test]
async fn request_filter_returns_404_when_no_route_matches() {
    let router = Arc::new(Router::new(vec![]).expect("empty routes"));
    let manager = Manager::new(&[]);
    let proxy = Proxy {
        router,
        upstream_manager: manager,
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
async fn request_filter_skips_selection_when_total_weight_zero() {
    let routes = vec![VirtualHost {
        host: "*".to_string(),
        paths: vec![Route {
            match_type: MatchType::Exact,
            path: "/api".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            destinations: vec![
                WeightedDestination {
                    upstream: "blue".to_string(),
                    weight: 0,
                },
                WeightedDestination {
                    upstream: "green".to_string(),
                    weight: 0,
                },
            ],
        }],
    }];
    let router = Arc::new(Router::new(routes).expect("routes"));
    let manager = Manager::new(&[upstream("blue", 8081), upstream("green", 8082)]);
    let proxy = Proxy {
        router,
        upstream_manager: manager,
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
