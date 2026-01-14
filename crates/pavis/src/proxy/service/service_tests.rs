use super::{Proxy, apply_route_headers, calculate_path_rewrite, route_path};
use crate::proxy::context::RouterContext;
use crate::state::{RuntimeState, RuntimeStateHandle};
use crate::telemetry::Telemetry;
use crate::upstream::Manager;
use arc_swap::ArcSwap;
use pavis_core::{
    AccessLogPolicy, ClientCert, ClientCertChain, ConnectTimeout, ConnectionLimit, Destination,
    Discovery, Duration, Endpoint, EndpointAddr, HeaderName, HeaderValue, Headers, HeadersPolicy,
    Host, Hostname, HttpVersion, IdleTimeout, LoadBalancer, Metrics, Path, PathMatch, Pool, Port,
    RetryPolicy, Rewrite, RewriteHost, RewritePath, RouteAction, ServiceName, SniName,
    Telemetry as RuntimeTelemetry, Timeout, TlsPolicy, Upstream, UpstreamCa, UpstreamId,
    UpstreamName, VirtualHost, Weight,
};
use pingora::http::ResponseHeader;
use pingora::prelude::{ProxyHttp, RequestHeader, Session};
use rustls::RootCertStore;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU16, NonZeroU32};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn apply_route_headers_populates_router_context() {
    let route = pavis_core::Route {
        matcher: PathMatch::Exact {
            path: Path("/".to_string()),
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
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
        upstream_name: None,
        request_headers: HeadersPolicy::Disabled.into(),
        response_headers: HeadersPolicy::Disabled.into(),
        sni_override: None,
        start_time: std::time::Instant::now(),
        client_identity: None,
        rbac_denied: false,
        // Observability fields
        upstream_timing: crate::proxy::context::UpstreamTiming::NotStarted,
        route_pattern: crate::proxy::context::RoutePattern::NotMatched,
        req_id: "test-req".to_string(),
        span: crate::proxy::context::TracingSpan::Disabled,
        runtime_state: None,
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
}

fn test_telemetry() -> Arc<Telemetry> {
    let (telemetry, _worker, _metrics_worker, _tracing_service) = Telemetry::new(
        &RuntimeTelemetry {
            level: pavis_core::LogLevel::Info,

            pingora: pavis_core::LogLevel::Info,

            service_name: ServiceName("svc".to_string()),

            metrics: Metrics::Disabled,

            access_log: AccessLogPolicy::Disabled,

            tracing: pavis_core::TracingPolicy::Disabled,
        },
        None,
    );

    Arc::new(telemetry)
}

fn test_ca_store() -> Arc<ArcSwap<RootCertStore>> {
    Arc::new(ArcSwap::from_pointee(RootCertStore::empty()))
}

#[test]
fn new_ctx_defaults_are_empty() {
    let manager = Manager::new(&[]).expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let before = Instant::now();
    let ctx = proxy.new_ctx();
    assert!(ctx.upstream_name.is_none());
    assert!(matches!(*ctx.request_headers, HeadersPolicy::Disabled));
    assert!(matches!(*ctx.response_headers, HeadersPolicy::Disabled));
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

fn upstream(name: &str, id: u16, port: u16) -> Upstream {
    Upstream {
        id: UpstreamId(NonZeroU16::new(id).unwrap()),
        name: UpstreamName(name.to_string()),
        discovery: Discovery::Static,
        balancer: LoadBalancer::Random,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit::Unlimited,
        },
        tls: TlsPolicy::Disabled,
        endpoints: vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(port).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    }
}

fn write_pem(path: &std::path::Path, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write pem");
}

fn build_self_signed_cert() -> (String, String) {
    let mut params = rcgen::CertificateParams::new(vec!["client".to_string()]).unwrap();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "client");
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    (key_pair.serialize_pem(), cert.pem())
}

fn mtls_upstream(
    name: &str,
    id: u16,
    port: u16,
    cert_path: PathBuf,
    key_path: PathBuf,
) -> Upstream {
    let mut upstream = upstream(name, id, port);
    upstream.tls = TlsPolicy::Enabled {
        verify: pavis_core::TlsVerify::Disabled,
        sni: SniName::Auto,
        cert: ClientCert::Enabled {
            cert_path: pavis_core::Path(cert_path.to_string_lossy().to_string()),
            key_path: pavis_core::Path(key_path.to_string_lossy().to_string()),
            chain: ClientCertChain::None,
        },
        ca: UpstreamCa::System,
    };
    upstream
}

#[tokio::test]
async fn request_filter_selects_weighted_destination() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Exact {
                path: Path("/api".to_string()),
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
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                },
                Destination {
                    upstream: UpstreamName("green".to_string()),
                    weight: Weight(NonZeroU16::new(2).unwrap()),
                },
            ]),
        }],
    }];
    let manager =
        Manager::new(&[upstream("blue", 1, 8081), upstream("green", 2, 8082)]).expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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
    let selected = ctx
        .upstream_name
        .as_ref()
        .map(|v| v.0.as_str())
        .expect("upstream selected");
    assert!(expected.contains(selected));
}

#[tokio::test]
async fn request_filter_returns_404_when_no_route_matches() {
    let manager = Manager::new(&[]).expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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
            matcher: PathMatch::Prefix {
                path: Path("/api".to_string()),
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
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }]),
        }],
    }];
    let manager = Manager::new(&[upstream("backend", 1, 8081)]).expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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
    assert_eq!(
        ctx.sni_override.as_ref().map(|v| v.0.as_str()),
        Some("rewrite.example.com")
    );
}

#[tokio::test]
async fn request_filter_skips_selection_when_no_destinations() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Exact {
                path: Path("/api".to_string()),
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
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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
        id: UpstreamId(NonZeroU16::new(1).unwrap()),
        name: UpstreamName("secure".to_string()),
        discovery: Discovery::Static,
        balancer: LoadBalancer::RoundRobin,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit::Unlimited,
        },
        tls: TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Full,
            sni: pavis_core::SniName::Name(Hostname("example.com".to_string())),
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        },
        endpoints: vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    }])
    .expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("secure".to_string()));

    let peer = proxy
        .upstream_peer(&mut session, &mut ctx)
        .await
        .expect("peer");
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "example.com");
}

#[tokio::test]
async fn upstream_peer_auto_sni_uses_dns_endpoint_host() {
    let manager = Manager::new(&[Upstream {
        id: UpstreamId(NonZeroU16::new(1).unwrap()),
        name: UpstreamName("dns".to_string()),
        discovery: Discovery::Logical,
        balancer: LoadBalancer::RoundRobin,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit::Unlimited,
        },
        tls: TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Full,
            sni: pavis_core::SniName::Auto,
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        },
        endpoints: vec![Endpoint {
            address: EndpointAddr::Dns {
                host: Hostname("localhost".to_string()),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    }])
    .expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("dns".to_string()));

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
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: Manager::new(&[]).expect("manager"),
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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
        .expect("filter");
    assert!(resp.headers.get("x-drop").is_none());
    assert_eq!(resp.headers.get("x-added").unwrap().to_str().unwrap(), "ok");
}

#[tokio::test]
async fn logging_handles_disabled_access_log() {
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: Manager::new(&[]).expect("manager"),
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    proxy.logging(&mut session, None, &mut ctx).await;
}

#[test]
fn test_calculate_path_rewrite() {
    let route_prefix = pavis_core::Route {
        matcher: PathMatch::Prefix {
            path: Path("/api".to_string()),
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

    // Prefix match
    let uri = calculate_path_rewrite(&route_prefix, "/api/foo", Some("q=1")).unwrap();
    assert_eq!(uri.path(), "/v2/foo");
    assert_eq!(uri.query(), Some("q=1"));

    // Prefix match without query
    let uri = calculate_path_rewrite(&route_prefix, "/api/foo", None).unwrap();
    assert_eq!(uri.path(), "/v2/foo");
    assert_eq!(uri.query(), None);

    let route_exact = pavis_core::Route {
        matcher: PathMatch::Exact {
            path: Path("/api".to_string()),
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

    // Exact match
    let uri = calculate_path_rewrite(&route_exact, "/api", None).unwrap();
    assert_eq!(uri.path(), "/v2");

    // Exact mismatch
    assert!(calculate_path_rewrite(&route_exact, "/api/foo", None).is_none());

    let route_regex = pavis_core::Route {
        matcher: PathMatch::Regex {
            path: Path("/api/.*".to_string()),
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

    // Regex match (currently returns None for rewrite)
    assert!(calculate_path_rewrite(&route_regex, "/api/foo", None).is_none());
}

#[test]
fn test_route_path_helper() {
    let r1 = pavis_core::Route {
        matcher: PathMatch::Prefix {
            path: Path("/p".to_string()),
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

    let r2 = pavis_core::Route {
        matcher: PathMatch::Exact {
            path: Path("/e".to_string()),
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
    assert_eq!(route_path(&r2), "/e");

    let r3 = pavis_core::Route {
        matcher: PathMatch::Regex {
            path: Path("/r".to_string()),
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
    assert_eq!(route_path(&r3), "/r");
}

#[tokio::test]
async fn upstream_peer_fails_when_no_upstream_in_ctx() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[]).expect("manager"),
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let res = proxy.upstream_peer(&mut session, &mut ctx).await;
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("No upstream selected")
    );
}

#[tokio::test]
async fn upstream_peer_fails_when_upstream_not_found() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[]).expect("manager"),
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("missing".to_string()));
    let res = proxy.upstream_peer(&mut session, &mut ctx).await;
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("Upstream not found in config")
    );
}

#[tokio::test]
async fn upstream_peer_fails_when_no_endpoints() {
    let manager = Manager::new(&[Upstream {
        id: UpstreamId(NonZeroU16::new(1).unwrap()),
        name: UpstreamName("empty".to_string()),
        discovery: Discovery::Static,
        balancer: LoadBalancer::Random,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit::Unlimited,
        },
        tls: TlsPolicy::Disabled,
        endpoints: vec![],
    }])
    .expect("manager");
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: manager,
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("empty".to_string()));
    let res = proxy.upstream_peer(&mut session, &mut ctx).await;
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("Upstream has no endpoints")
    );
}

#[test]
fn test_calculate_path_rewrite_unmatched_prefix() {
    let route = pavis_core::Route {
        matcher: PathMatch::Prefix {
            path: Path("/api".to_string()),
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

    // Path does not start with /api
    assert!(calculate_path_rewrite(&route, "/other", None).is_none());
}

#[tokio::test]
async fn test_proxy_logging_with_upstream() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[]).expect("manager"),
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("backend".to_string()));
    proxy.logging(&mut session, None, &mut ctx).await;
}

#[tokio::test]
async fn request_filter_handles_redirect_action() {
    // Test that RouteAction::Redirect returns proper redirect response
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Exact {
                path: Path("/old".to_string()),
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

    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).unwrap()),
        upstream_manager: Manager::new(&[]).expect("manager"),
    };
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /old HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let result = proxy.request_filter(&mut session, &mut ctx).await;

    assert!(result.is_ok());
    assert!(result.unwrap()); // Should stop processing

    // Read the response
    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("301"));
    assert!(response.contains("Location: https://example.com/new"));
}

#[tokio::test]
async fn request_filter_handles_direct_action() {
    // Test that RouteAction::Direct returns custom response
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Exact {
                path: Path("/health".to_string()),
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

    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).unwrap()),
        upstream_manager: Manager::new(&[]).expect("manager"),
    };
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /health HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let result = proxy.request_filter(&mut session, &mut ctx).await;

    assert!(result.is_ok());
    assert!(result.unwrap()); // Should stop processing

    // Read the response
    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("200"));
    assert!(response.contains("Content-Type: text/plain"));
    assert!(response.contains("OK"));
}

#[tokio::test]
async fn request_filter_redirect_with_different_status_codes() {
    // Test 302 redirect
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Prefix {
                path: Path("/temp".to_string()),
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

    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).unwrap()),
        upstream_manager: Manager::new(&[]).expect("manager"),
    };
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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
    // Test direct response with 404 status
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Prefix {
                path: Path("/gone".to_string()),
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

    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).unwrap()),
        upstream_manager: Manager::new(&[]).expect("manager"),
    };
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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
        matcher: PathMatch::Prefix {
            path: Path("/api/v1".to_string()),
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

    // Test with complex query string
    let uri = calculate_path_rewrite(
        &route,
        "/api/v1/users",
        Some("id=123&filter=active&sort=name"),
    )
    .unwrap();
    assert_eq!(uri.path(), "/v2/users");
    assert_eq!(uri.query(), Some("id=123&filter=active&sort=name"));

    // Test with empty query string
    let uri = calculate_path_rewrite(&route, "/api/v1/users", Some("")).unwrap();
    assert_eq!(uri.path(), "/v2/users");
    assert_eq!(uri.query(), Some(""));

    // Test with special characters in query
    let uri =
        calculate_path_rewrite(&route, "/api/v1/search", Some("q=hello%20world&page=1")).unwrap();
    assert_eq!(uri.path(), "/v2/search");
    assert_eq!(uri.query(), Some("q=hello%20world&page=1"));
}

#[tokio::test]
async fn test_upstream_request_filter() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[]).expect("manager"),
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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
async fn test_upstream_peer_tls_verify_variants() {
    let mut upstream_base = upstream("verify", 1, 8080);
    upstream_base.tls = TlsPolicy::Enabled {
        verify: pavis_core::TlsVerify::Disabled,
        sni: SniName::Name(Hostname("example.com".to_string())),
        cert: pavis_core::ClientCert::Disabled,
        ca: UpstreamCa::System,
    };

    let test_modes = [
        (pavis_core::TlsVerify::Disabled, false, false),
        (pavis_core::TlsVerify::CaOnly, false, true),
        (pavis_core::TlsVerify::Full, true, true),
    ];

    for (mode, verify_host, verify_cert) in test_modes {
        let mut u = upstream_base.clone();
        if let TlsPolicy::Enabled { verify, .. } = &mut u.tls {
            *verify = mode;
        }

        let proxy = Proxy {
            state: Arc::new(RuntimeStateHandle::new(RuntimeState {
                config: RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: Manager::new(&[u]).expect("manager"),
            })),
            telemetry: test_telemetry(),
            ca_store: test_ca_store(),
        };

        let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
        let mut ctx = proxy.new_ctx();
        ctx.upstream_name = Some(UpstreamName("verify".to_string()));

        let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
        assert_eq!(peer.options.verify_hostname, verify_host);
        assert_eq!(peer.options.verify_cert, verify_cert);
    }
}

#[tokio::test]
async fn upstream_peer_sets_client_cert_key() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "pavis_upstream_client_cert_{}",
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let cert_path = dir.join("client.pem");
    let key_path = dir.join("client.key");

    let (client_key_pem, client_cert_pem) = build_self_signed_cert();
    write_pem(&cert_path, client_cert_pem.as_bytes());
    write_pem(&key_path, client_key_pem.as_bytes());

    let upstream = mtls_upstream("secure", 1, 8443, cert_path, key_path);
    let manager = Manager::new(&[upstream]).expect("manager");
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).expect("empty routes")),
            upstream_manager: manager,
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("secure".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.client_cert_key.is_some());
}

#[tokio::test]
async fn test_request_filter_direct_response_with_headers() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Exact {
                path: Path("/direct".to_string()),
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
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(routes).unwrap()),
            upstream_manager: Manager::new(&[]).expect("manager"),
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, mut client) = session_for_request(b"GET /direct HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    proxy.request_filter(&mut session, &mut ctx).await.unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("x-direct: true"));
    assert!(response.contains("Direct"));
}

#[tokio::test]
async fn request_filter_applies_rewrite_and_preserves_query() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Prefix {
                path: Path("/old-api".to_string()),
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
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }]),
        }],
    }];
    let manager = Manager::new(&[upstream("backend", 1, 8081)]).expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
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

    let header = session.as_downstream().req_header();
    assert_eq!(header.uri.path(), "/new-api/resource");
    assert_eq!(header.uri.query(), Some("filter=active&limit=10"));
}

#[tokio::test]
async fn upstream_peer_dns_supported() {
    let manager = Manager::new(&[Upstream {
        id: UpstreamId(NonZeroU16::new(1).unwrap()),
        name: UpstreamName("dns-upstream".to_string()),
        discovery: Discovery::Logical,
        balancer: LoadBalancer::RoundRobin,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit::Unlimited,
        },
        tls: TlsPolicy::Disabled,
        endpoints: vec![Endpoint {
            address: EndpointAddr::Dns {
                host: Hostname("example.com".to_string()),
                port: Port(NonZeroU16::new(80).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    }])
    .expect("manager");
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: manager,
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("dns-upstream".to_string()));
    let res = proxy.upstream_peer(&mut session, &mut ctx).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn upstream_peer_tls_and_pool_variants() {
    let mut upstream = upstream("variants", 1, 8080);
    upstream.tls = TlsPolicy::Enabled {
        verify: pavis_core::TlsVerify::CaOnly,
        sni: pavis_core::SniName::Name(Hostname("custom.sni".to_string())),
        cert: pavis_core::ClientCert::Disabled,
        ca: UpstreamCa::System,
    };
    upstream.protocol = HttpVersion::H2;
    upstream.pool.idle = IdleTimeout::Enabled(Duration(NonZeroU32::new(1000).unwrap()));
    upstream.pool.connect = ConnectTimeout::Enabled(Duration(NonZeroU32::new(2000).unwrap()));

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[upstream]).expect("manager"),
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("variants".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "custom.sni");
    assert_eq!(
        peer.options.idle_timeout,
        Some(std::time::Duration::from_millis(1000))
    );
    assert_eq!(
        peer.options.connection_timeout,
        Some(std::time::Duration::from_millis(2000))
    );
}

#[tokio::test]
async fn upstream_peer_sni_fallback_warning() {
    // Configures TLS upstream with Auto SNI, but request has no Host header
    let manager = Manager::new(&[Upstream {
        id: UpstreamId(NonZeroU16::new(1).unwrap()),
        name: UpstreamName("tls-no-sni".to_string()),
        discovery: Discovery::Static,
        balancer: LoadBalancer::RoundRobin,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit::Unlimited,
        },
        tls: TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Disabled,
            sni: pavis_core::SniName::Auto,
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        },
        endpoints: vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    }])
    .expect("manager");

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: manager,
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    // Request without Host header
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("tls-no-sni".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "");
}

#[tokio::test]
async fn upstream_peer_sni_override_prevents_fallback() {
    let manager = Manager::new(&[Upstream {
        id: UpstreamId(NonZeroU16::new(1).unwrap()),
        name: UpstreamName("tls-auto".to_string()),
        discovery: Discovery::Static,
        balancer: LoadBalancer::RoundRobin,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit::Unlimited,
        },
        tls: TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Disabled,
            sni: pavis_core::SniName::Auto,
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        },
        endpoints: vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    }])
    .expect("manager");

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: manager,
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("tls-auto".to_string()));

    // Set explicit override (e.g. from Host header rewrite)
    ctx.sni_override = Some(Hostname("overridden.com".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "overridden.com");
}

#[tokio::test]
async fn upstream_peer_explicit_sni_prevents_fallback() {
    let manager = Manager::new(&[Upstream {
        id: UpstreamId(NonZeroU16::new(1).unwrap()),
        name: UpstreamName("tls-explicit".to_string()),
        discovery: Discovery::Static,
        balancer: LoadBalancer::RoundRobin,
        protocol: HttpVersion::H1,
        pool: Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit::Unlimited,
        },
        tls: TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Disabled,
            sni: pavis_core::SniName::Name(Hostname("explicit.com".to_string())),
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        },
        endpoints: vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    }])
    .expect("manager");

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            config: RuntimeState::default().config,
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: manager,
        })),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("tls-explicit".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "explicit.com");
}

#[test]
fn test_calculate_path_rewrite_invalid_uri() {
    let route = pavis_core::Route {
        matcher: PathMatch::Prefix {
            path: Path("/".to_string()),
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled.into(),
        response_headers: HeadersPolicy::Disabled.into(),
        principal: pavis_core::Principal::Any,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/".to_string()),
                to: Path("/\0".to_string()), // Invalid character for URI
            },
            host: RewriteHost::Disabled,
        },
        action: RouteAction::Forward(vec![]),
    };
    assert!(calculate_path_rewrite(&route, "/", None).is_none());
}

#[test]
fn test_is_authorized_principal_variants() {
    let any = pavis_core::Principal::Any;
    let auth = pavis_core::Principal::Authenticated {
        spiffe: "spiffe://cluster/ns/prod/sa/app1".to_string(),
    };
    let prefix = pavis_core::Principal::Prefix {
        prefix: "spiffe://cluster/ns/prod/sa/".to_string(),
    };

    assert!(super::is_authorized(&any, None));
    assert!(super::is_authorized(
        &any,
        Some("spiffe://example.org/ns/foo/sa/bar")
    ));

    assert!(super::is_authorized(
        &auth,
        Some("spiffe://cluster/ns/prod/sa/app1")
    ));
    assert!(!super::is_authorized(
        &auth,
        Some("spiffe://cluster/ns/prod/sa/app2")
    ));
    assert!(!super::is_authorized(&auth, None));

    assert!(super::is_authorized(
        &prefix,
        Some("spiffe://cluster/ns/prod/sa/app1")
    ));
    assert!(!super::is_authorized(
        &prefix,
        Some("spiffe://cluster/ns/dev/sa/app1")
    ));
    assert!(!super::is_authorized(&prefix, None));
}

#[tokio::test]
async fn request_filter_denies_when_principal_not_any() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Exact {
                path: Path("/secure".to_string()),
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Authenticated {
                spiffe: "spiffe://cluster/ns/prod/sa/app1".to_string(),
            },
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![Destination {
                upstream: UpstreamName("backend".to_string()),
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }]),
        }],
    }];
    let manager = Manager::new(&[upstream("backend", 1, 8081)]).expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, mut client) =
        session_for_request(b"GET /secure HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    let should_respond = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("request filter");
    assert!(should_respond);
    assert!(ctx.upstream_name.is_none());

    let mut buf = [0u8; 256];
    let read = client.read(&mut buf).await.expect("read response");
    let body = String::from_utf8_lossy(&buf[..read]);
    assert!(body.contains("403"));
}

#[tokio::test]
async fn request_filter_allows_with_matching_identity() {
    let routes = vec![VirtualHost {
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Exact {
                path: Path("/secure".to_string()),
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Authenticated {
                spiffe: "spiffe://cluster/ns/prod/sa/app1".to_string(),
            },
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![Destination {
                upstream: UpstreamName("backend".to_string()),
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }]),
        }],
    }];
    let manager = Manager::new(&[upstream("backend", 1, 8081)]).expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(crate::router::Router::new(routes).expect("routes")),
        upstream_manager: manager,
    };
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
        ca_store: test_ca_store(),
    };

    let (mut session, _client) =
        session_for_request(b"GET /secure HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.client_identity = Some("spiffe://cluster/ns/prod/sa/app1".to_string());

    let should_respond = proxy
        .request_filter(&mut session, &mut ctx)
        .await
        .expect("request filter");
    assert!(!should_respond);
    assert_eq!(
        ctx.upstream_name.as_ref().map(|v| v.0.as_str()),
        Some("backend")
    );
}
