use super::{Proxy, apply_route_headers, calculate_path_rewrite, route_path};
use crate::proxy::context::RouterContext;
use crate::state::{RuntimeState, RuntimeStateHandle};
use crate::telemetry::Telemetry;
use crate::upstream::Manager;
use pavis_core::{
    AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Discovery, Duration, Endpoint,
    EndpointAddr, HeaderName, HeaderValue, Headers, HeadersPolicy, Host, Hostname, HttpVersion,
    IdleTimeout, LoadBalancer, Metrics, Path, PathMatch, Pool, Port, RetryPolicy, Rewrite,
    RewriteHost, RewritePath, ServiceName, Telemetry as RuntimeTelemetry, Timeout, TlsPolicy,
    Upstream, UpstreamId, UpstreamName, VirtualHost, Weight,
};
use pingora::http::ResponseHeader;
use pingora::proxy::ProxyHttp;
use pingora::proxy::Session;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU16, NonZeroU32};
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
        },
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
        },
        rewrite: Rewrite {
            path: RewritePath::Disabled,
            host: RewriteHost::Disabled,
        },
        destinations: vec![Destination {
            upstream: UpstreamName("backend".to_string()),
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }],
    };
    let mut ctx = RouterContext {
        upstream_name: None,
        request_headers: HeadersPolicy::Disabled,
        response_headers: HeadersPolicy::Disabled,
        sni_override: None,
        start_time: std::time::Instant::now(),
    };

    apply_route_headers(&mut ctx, &route);

    assert!(matches!(ctx.request_headers, HeadersPolicy::Enabled { .. }));
    assert!(matches!(
        ctx.response_headers,
        HeadersPolicy::Enabled { .. }
    ));
}

fn test_telemetry() -> Arc<Telemetry> {
    let (telemetry, _worker) = Telemetry::new(&RuntimeTelemetry {
        level: pavis_core::LogLevel::Info,
        pingora: pavis_core::LogLevel::Info,
        service_name: ServiceName("svc".to_string()),
        metrics: Metrics::Disabled,
        access_log: AccessLogPolicy::Disabled,
        tracing: pavis_core::TracingPolicy::Disabled,
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
    assert!(matches!(ctx.request_headers, HeadersPolicy::Disabled));
    assert!(matches!(ctx.response_headers, HeadersPolicy::Disabled));
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
            request_headers: HeadersPolicy::Disabled,
            response_headers: HeadersPolicy::Disabled,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            destinations: vec![
                Destination {
                    upstream: UpstreamName("blue".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                },
                Destination {
                    upstream: UpstreamName("green".to_string()),
                    weight: Weight(NonZeroU16::new(2).unwrap()),
                },
            ],
        }],
    }];
    let manager = Manager::new(&[upstream("blue", 1, 8081), upstream("green", 2, 8082)]);
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
    let selected = ctx
        .upstream_name
        .as_ref()
        .map(|v| v.0.as_str())
        .expect("upstream selected");
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
        host: Host("*".to_string()),
        paths: vec![pavis_core::Route {
            matcher: PathMatch::Prefix {
                path: Path("/api".to_string()),
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled,
            response_headers: HeadersPolicy::Disabled,
            rewrite: Rewrite {
                path: RewritePath::Prefix {
                    from: Path("/api".to_string()),
                    to: Path("/v2".to_string()),
                },
                host: RewriteHost::Literal {
                    host: Hostname("rewrite.example.com".to_string()),
                },
            },
            destinations: vec![Destination {
                upstream: UpstreamName("backend".to_string()),
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        }],
    }];
    let manager = Manager::new(&[upstream("backend", 1, 8081)]);
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
            request_headers: HeadersPolicy::Disabled,
            response_headers: HeadersPolicy::Disabled,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            destinations: Vec::new(),
        }],
    }];
    let manager = Manager::new(&[upstream("blue", 1, 8081), upstream("green", 2, 8082)]);
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
            verify_mode: pavis_core::TlsVerify::CertAndHost,
            sni: pavis_core::SniName::Auto,
        },
        endpoints: vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
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
    ctx.upstream_name = Some(UpstreamName("secure".to_string()));

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
    };

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

#[test]
fn test_calculate_path_rewrite() {
    let route_prefix = pavis_core::Route {
        matcher: PathMatch::Prefix {
            path: Path("/api".to_string()),
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled,
        response_headers: HeadersPolicy::Disabled,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        destinations: vec![],
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
        request_headers: HeadersPolicy::Disabled,
        response_headers: HeadersPolicy::Disabled,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        destinations: vec![],
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
        request_headers: HeadersPolicy::Disabled,
        response_headers: HeadersPolicy::Disabled,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        destinations: vec![],
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
        request_headers: HeadersPolicy::Disabled,
        response_headers: HeadersPolicy::Disabled,
        rewrite: Rewrite {
            path: RewritePath::Disabled,
            host: RewriteHost::Disabled,
        },
        destinations: vec![],
    };
    assert_eq!(route_path(&r1), "/p");

    let r2 = pavis_core::Route {
        matcher: PathMatch::Exact {
            path: Path("/e".to_string()),
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled,
        response_headers: HeadersPolicy::Disabled,
        rewrite: Rewrite {
            path: RewritePath::Disabled,
            host: RewriteHost::Disabled,
        },
        destinations: vec![],
    };
    assert_eq!(route_path(&r2), "/e");

    let r3 = pavis_core::Route {
        matcher: PathMatch::Regex {
            path: Path("/r".to_string()),
        },
        timeout: Timeout::Disabled,
        retry: RetryPolicy::Disabled,
        request_headers: HeadersPolicy::Disabled,
        response_headers: HeadersPolicy::Disabled,
        rewrite: Rewrite {
            path: RewritePath::Disabled,
            host: RewriteHost::Disabled,
        },
        destinations: vec![],
    };
    assert_eq!(route_path(&r3), "/r");
}

#[tokio::test]
async fn upstream_peer_fails_when_no_upstream_in_ctx() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[]),
        })),
        telemetry: test_telemetry(),
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
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[]),
        })),
        telemetry: test_telemetry(),
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
    }]);
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: manager,
        })),
        telemetry: test_telemetry(),
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
        request_headers: HeadersPolicy::Disabled,
        response_headers: HeadersPolicy::Disabled,
        rewrite: Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        },
        destinations: vec![],
    };

    // Path does not start with /api
    assert!(calculate_path_rewrite(&route, "/other", None).is_none());
}

#[tokio::test]
async fn test_proxy_logging_with_upstream() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState {
            router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
            upstream_manager: Manager::new(&[]),
        })),
        telemetry: test_telemetry(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.upstream_name = Some(UpstreamName("backend".to_string()));
    proxy.logging(&mut session, None, &mut ctx).await;
}
