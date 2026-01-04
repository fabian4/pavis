use super::{Proxy, apply_route_headers};
use crate::proxy::context::RouterContext;
use crate::state::{RuntimeState, RuntimeStateHandle};
use crate::telemetry::Telemetry;
use crate::upstream::Manager;
use pavis_core::{
    AccessLogConfig, ConnectionPoolConfig, DiscoveryType, Endpoint, EndpointAddress, HeaderAction,
    HeaderActionType, HeaderOperations, HttpVersion, LoadBalancer, MatchType, Route,
    TelemetryConfig, Upstream, UpstreamTlsConfig, VirtualHost, WeightedDestination,
};
use pingora::http::ResponseHeader;
use pingora::proxy::ProxyHttp;
use pingora::proxy::Session;
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
            actions: vec![
                HeaderAction {
                    key: "x-req".to_string(),
                    value: Some("1".to_string()),
                    action: HeaderActionType::Set,
                },
                HeaderAction {
                    key: "x-remove".to_string(),
                    value: None,
                    action: HeaderActionType::Remove,
                },
            ],
        }),
        response_headers: Some(HeaderOperations {
            actions: vec![HeaderAction {
                key: "x-resp".to_string(),
                value: Some("ok".to_string()),
                action: HeaderActionType::Set,
            }],
        }),
        rewrite: None,
        destinations: vec![WeightedDestination {
            upstream: "backend".to_string(),
            weight: 1,
        }],
    };
    let mut ctx = RouterContext {
        upstream_name: None,
        request_headers: None,
        response_headers: None,
        sni_override: None,
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
    let manager = Manager::new(&[]);
    let state = RuntimeState {
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let before = Instant::now();
    let ctx = proxy.new_ctx();
    assert!(ctx.upstream_name.is_none());
    assert!(ctx.request_headers.is_none());
    assert!(ctx.response_headers.is_none());
    assert!(ctx.sni_override.is_none());
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
        discovery_type: DiscoveryType::Static,
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![Endpoint {
            address: EndpointAddress::Ip(std::net::SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                port,
            )),
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
            rewrite: None,
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
    let manager = Manager::new(&[upstream("blue", 8081), upstream("green", 8082)]);
    let state = RuntimeState {
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
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

    let expected: HashSet<&str> = ["blue", "green"].into_iter().collect();
    let selected = ctx.upstream_name.as_deref().expect("upstream selected");
    assert!(expected.contains(selected));
}

#[tokio::test]
async fn request_filter_returns_404_when_no_route_matches() {
    let manager = Manager::new(&[]);
    let state = RuntimeState {
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: manager,
    };
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
        host: "*".to_string(),
        paths: vec![Route {
            match_type: MatchType::Prefix,
            path: "/api".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            rewrite: Some(pavis_core::RewritePolicy {
                path_prefix_rewrite: Some("/v2".to_string()),
                host_rewrite_literal: Some("rewrite.example.com".to_string()),
            }),
            destinations: vec![WeightedDestination {
                upstream: "backend".to_string(),
                weight: 1,
            }],
        }],
    }];
    let manager = Manager::new(&[upstream("backend", 8081)]);
    let state = RuntimeState {
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
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

    let header = session.as_downstream().req_header();
    assert_eq!(header.uri.path(), "/v2/widgets");
    assert_eq!(header.uri.query(), Some("id=1"));
    assert_eq!(
        header.headers.get("Host").unwrap().to_str().unwrap(),
        "rewrite.example.com"
    );
    assert_eq!(ctx.sni_override.as_deref(), Some("rewrite.example.com"));
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
            rewrite: None,
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
    let manager = Manager::new(&[upstream("blue", 8081), upstream("green", 8082)]);
    let state = RuntimeState {
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
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

#[tokio::test]
async fn upstream_peer_defaults_sni() {
    let manager = Manager::new(&[Upstream {
        name: "secure".to_string(),
        discovery_type: DiscoveryType::Static,
        load_balancer: LoadBalancer::RoundRobin,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: Some(UpstreamTlsConfig {
            enabled: true,
            verify_hostname: true,
            verify_cert: true,
            sni: None,
        }),
        endpoints: vec![Endpoint {
            address: EndpointAddress::Ip(std::net::SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                8443,
            )),
            weight: 1,
        }],
    }]);
    let state = RuntimeState {
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some("secure".to_string());

    let peer = proxy
        .upstream_peer(&mut session, &mut ctx)
        .await
        .expect("peer");
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "localhost");
}

#[tokio::test]
async fn upstream_response_filter_applies_headers() {
    let state = RuntimeState {
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: Manager::new(&[]),
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let mut ctx = proxy.new_ctx();
    ctx.response_headers = Some(HeaderOperations {
        actions: vec![
            HeaderAction {
                key: "x-added".to_string(),
                value: Some("ok".to_string()),
                action: HeaderActionType::Set,
            },
            HeaderAction {
                key: "x-drop".to_string(),
                value: None,
                action: HeaderActionType::Remove,
            },
        ],
    });

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut resp = ResponseHeader::build(200, None).expect("resp");
    resp.insert_header("x-drop", "gone").expect("header");

    proxy
        .upstream_response_filter(&mut session, &mut resp, &mut ctx)
        .expect("filter");
    assert!(resp.headers.get("x-drop").is_none());
    assert_eq!(resp.headers.get("x-added").unwrap().to_str().unwrap(), "ok");
}

#[tokio::test]
async fn logging_handles_disabled_access_log() {
    let state = RuntimeState {
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: Manager::new(&[]),
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    proxy.logging(&mut session, None, &mut ctx).await;
}
