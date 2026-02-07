mod common;

use common::*;
use pavis::proxy::service::test_exports::Proxy;
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis::upstream::Manager;
use pavis_core::{
    ConnectTimeout, ConnectionLimit, Discovery, Duration, Endpoint, EndpointAddr, Hostname,
    HttpVersion, IdleTimeout, LoadBalancer, Pool, PoolQueue, Port, SniName, TlsPolicy, Upstream,
    UpstreamBuilder, UpstreamCa, UpstreamId, UpstreamName, Weight,
};
use pingora::prelude::ProxyHttp;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

#[tokio::test]
async fn upstream_peer_defaults_sni() {
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("secure".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::RoundRobin)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Full,
            sni: pavis_core::SniName::Name(Hostname("example.com".to_string())),
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        })
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream");
    let manager = Manager::new(&[upstream_cfg]).expect("manager");
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

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
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
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("dns".to_string()))
        .discovery(Discovery::Logical)
        .balancer(LoadBalancer::RoundRobin)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Full,
            sni: pavis_core::SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        })
        .add_endpoint(Endpoint {
            address: EndpointAddr::Dns {
                host: Hostname("localhost".to_string()),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream");
    let upstreams = vec![upstream_cfg];
    let manager = Manager::new(&upstreams).expect("manager");
    materialize_test_upstreams(&manager, &upstreams);
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

    let (mut session, _client) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
    ctx.upstream_name = Some(UpstreamName("dns".to_string()));

    let peer = proxy
        .upstream_peer(&mut session, &mut ctx)
        .await
        .expect("peer");
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "localhost");
}

#[tokio::test]
async fn upstream_peer_fails_when_no_upstream_in_ctx() {
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
            RuntimeState::default().config,
            Arc::new(pavis::router::Router::new(vec![]).unwrap()),
            Manager::new(&[]).expect("manager"),
        ))),
        telemetry: test_telemetry(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
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
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
            RuntimeState::default().config,
            Arc::new(pavis::router::Router::new(vec![]).unwrap()),
            Manager::new(&[]).expect("manager"),
        ))),
        telemetry: test_telemetry(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
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
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("empty".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(TlsPolicy::Disabled)
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
    ctx.upstream_name = Some(UpstreamName("empty".to_string()));
    let res = proxy.upstream_peer(&mut session, &mut ctx).await;
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("Upstream has no endpoints")
    );
}

#[tokio::test]
async fn upstream_peer_returns_503_when_pool_full() {
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("limited".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit(NonZeroU32::new(1).unwrap()),
            queue: PoolQueue {
                capacity: 0,
                timeout_ms: 0,
            },
            tcp_keepalive: None,
            tcp_nodelay: None,
            recv_buffer_size: None,
        })
        .tls(TlsPolicy::Disabled)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8001).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
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

    let (mut session_one, _client_one) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx_one = proxy.new_ctx();
    pin_runtime_state(&mut ctx_one, &proxy);
    ctx_one.upstream_name = Some(UpstreamName("limited".to_string()));
    proxy
        .upstream_peer(&mut session_one, &mut ctx_one)
        .await
        .expect("first peer");

    let (mut session_two, _client_two) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx_two = proxy.new_ctx();
    pin_runtime_state(&mut ctx_two, &proxy);
    ctx_two.upstream_name = Some(UpstreamName("limited".to_string()));
    let err = proxy
        .upstream_peer(&mut session_two, &mut ctx_two)
        .await
        .expect_err("pool full");
    assert!(
        err.to_string()
            .contains("ERR_UPSTREAM_POOL_FULL: connection pool is full")
    );
    ctx_one.pool_permit.take();
    ctx_one.circuit_breaker_permit.take();
}

#[tokio::test]
async fn upstream_peer_returns_503_when_pool_wait_times_out() {
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("queued".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit(NonZeroU32::new(1).unwrap()),
            queue: PoolQueue {
                capacity: 1,
                timeout_ms: 25,
            },
            tcp_keepalive: None,
            tcp_nodelay: None,
            recv_buffer_size: None,
        })
        .tls(TlsPolicy::Disabled)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8002).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
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

    let (mut session_one, _client_one) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx_one = proxy.new_ctx();
    pin_runtime_state(&mut ctx_one, &proxy);
    ctx_one.upstream_name = Some(UpstreamName("queued".to_string()));
    proxy
        .upstream_peer(&mut session_one, &mut ctx_one)
        .await
        .expect("first peer");

    let (mut session_two, _client_two) =
        session_for_request(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    let mut ctx_two = proxy.new_ctx();
    pin_runtime_state(&mut ctx_two, &proxy);
    ctx_two.upstream_name = Some(UpstreamName("queued".to_string()));
    let err = proxy
        .upstream_peer(&mut session_two, &mut ctx_two)
        .await
        .expect_err("pool timeout");
    assert!(
        err.to_string()
            .contains("ERR_UPSTREAM_POOL_FULL: connection pool wait timed out")
    );
    ctx_one.pool_permit.take();
    ctx_one.circuit_breaker_permit.take();
}

#[tokio::test]
async fn upstream_peer_errors_without_snapshot() {
    let upstream_cfg = upstream("backend", 1, 8080);
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
            RuntimeState::default().config,
            Arc::new(pavis::router::Router::new(vec![]).unwrap()),
            Manager::new(&[upstream_cfg]).expect("manager"),
        ))),
        telemetry: test_telemetry(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.set_request_id("req-missing".parse().unwrap());
    ctx.upstream_name = Some(UpstreamName("backend".to_string()));
    ctx.route_pattern = pavis::proxy::context::RoutePattern::Matched {
        pattern: Arc::from("/missing"),
    };

    let err = proxy
        .upstream_peer(&mut session, &mut ctx)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("missing runtime snapshot"));
    assert!(msg.contains("request_id=req-missing"));
    assert!(msg.contains("route=/missing"));
    assert!(msg.contains("upstream=backend"));
}

#[tokio::test]
async fn upstream_peer_uses_pinned_state_over_latest() {
    let proxy_state = RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(vec![]).unwrap()),
        Manager::new(&[upstream("new", 1, 8080)]).expect("manager"),
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(proxy_state)),
        telemetry: test_telemetry(),
    };

    let pinned_state = Arc::new(RuntimeState::with_components(
        RuntimeState::default().config,
        Arc::new(pavis::router::Router::new(vec![]).unwrap()),
        Manager::new(&[]).expect("manager"),
    ));

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    ctx.runtime_state = Some(pinned_state);
    ctx.upstream_name = Some(UpstreamName("new".to_string()));

    let err = proxy
        .upstream_peer(&mut session, &mut ctx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Upstream not found in config"));
}

#[tokio::test]
async fn test_upstream_peer_tls_verify_variants() {
    let mut upstream_base = upstream("verify", 1, 8080);
    upstream_base.tls = TlsPolicy::Enabled {
        verify: pavis_core::TlsVerify::Disabled,
        sni: SniName::Name(Hostname("example.com".to_string())),
        canonical_sni: pavis_core::CanonicalSni::Disabled,
        reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
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
            state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
                RuntimeState::default().config,
                Arc::new(pavis::router::Router::new(vec![]).unwrap()),
                Manager::new(&[u]).expect("manager"),
            ))),
            telemetry: test_telemetry(),
        };

        let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
        let mut ctx = proxy.new_ctx();
        pin_runtime_state(&mut ctx, &proxy);
        ctx.upstream_name = Some(UpstreamName("verify".to_string()));

        let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
        assert_eq!(peer.options.verify_hostname, verify_host);
        assert_eq!(peer.options.verify_cert, verify_cert);
    }
}

#[tokio::test]
async fn upstream_peer_sets_client_cert_key() {
    let dir = tempfile::tempdir().expect("create temp dir");

    let cert_path = dir.path().join("client.pem");
    let key_path = dir.path().join("client.key");

    let (client_key_pem, client_cert_pem) = build_self_signed_cert();
    write_pem(&cert_path, client_cert_pem.as_bytes());
    write_pem(&key_path, client_key_pem.as_bytes());

    let upstream_cfg = mtls_upstream("secure", 1, 8443, cert_path, key_path);
    let manager = Manager::new(&[upstream_cfg]).expect("manager");
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
            RuntimeState::default().config,
            Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
            manager,
        ))),
        telemetry: test_telemetry(),
    };

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
    ctx.upstream_name = Some(UpstreamName("secure".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.client_cert_key.is_some());
}

#[tokio::test]
async fn upstream_peer_dns_supported() {
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("dns-upstream".to_string()))
        .discovery(Discovery::Logical)
        .balancer(LoadBalancer::RoundRobin)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(TlsPolicy::Disabled)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Dns {
                host: Hostname("example.com".to_string()),
                port: Port(NonZeroU16::new(80).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream");
    let upstreams = vec![upstream_cfg];
    let manager = Manager::new(&upstreams).expect("manager");
    materialize_test_upstreams(&manager, &upstreams);
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
    ctx.upstream_name = Some(UpstreamName("dns-upstream".to_string()));
    let res = proxy.upstream_peer(&mut session, &mut ctx).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn upstream_peer_tls_and_pool_variants() {
    let mut upstream_cfg = upstream("variants", 1, 8080);
    upstream_cfg.tls = TlsPolicy::Enabled {
        verify: pavis_core::TlsVerify::CaOnly,
        sni: pavis_core::SniName::Name(Hostname("custom.sni".to_string())),
        canonical_sni: pavis_core::CanonicalSni::Disabled,
        reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
        cert: pavis_core::ClientCert::Disabled,
        ca: UpstreamCa::System,
    };
    upstream_cfg.protocol = HttpVersion::H2;
    upstream_cfg.pool.idle = IdleTimeout::Enabled(Duration(NonZeroU32::new(1000).unwrap()));
    upstream_cfg.pool.connect = ConnectTimeout::Enabled(Duration(NonZeroU32::new(2000).unwrap()));

    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(RuntimeState::with_components(
            RuntimeState::default().config,
            Arc::new(pavis::router::Router::new(vec![]).unwrap()),
            Manager::new(&[upstream_cfg]).expect("manager"),
        ))),
        telemetry: test_telemetry(),
    };
    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
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
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("tls-no-sni".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::RoundRobin)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Disabled,
            sni: pavis_core::SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        })
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
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
    ctx.upstream_name = Some(UpstreamName("tls-no-sni".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "");
}

#[tokio::test]
async fn upstream_peer_sni_override_prevents_fallback() {
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("tls-auto".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::RoundRobin)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Disabled,
            sni: pavis_core::SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        })
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
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
    ctx.upstream_name = Some(UpstreamName("tls-auto".to_string()));

    ctx.sni_override = Some(Hostname("overridden.com".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "overridden.com");
}

#[tokio::test]
async fn upstream_peer_explicit_sni_prevents_fallback() {
    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("tls-explicit".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::RoundRobin)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
            ..Pool::default()
        })
        .tls(TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Disabled,
            sni: pavis_core::SniName::Name(Hostname("explicit.com".to_string())),
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: UpstreamCa::System,
        })
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8443).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
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
    ctx.upstream_name = Some(UpstreamName("tls-explicit".to_string()));

    let peer = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.is_tls());
    assert_eq!(peer.sni, "explicit.com");
}

fn materialize_test_upstreams(manager: &Manager, upstreams: &[Upstream]) {
    for upstream in upstreams {
        if let Some(cluster) = manager.get(upstream.name.0.as_str()) {
            let endpoints = upstream
                .endpoints
                .iter()
                .flat_map(resolve_endpoint_for_tests)
                .collect::<Vec<_>>();
            cluster.update_endpoints(endpoints);
        }
    }
}

fn resolve_endpoint_for_tests(endpoint: &Endpoint) -> Vec<Endpoint> {
    match &endpoint.address {
        EndpointAddr::Ip { .. } => vec![endpoint.clone()],
        EndpointAddr::Dns { host, port } => (host.0.as_str(), port.0.get())
            .to_socket_addrs()
            .expect("failed to resolve test DNS endpoint")
            .map(|addr| Endpoint {
                address: EndpointAddr::Ip {
                    address: addr.ip(),
                    port: *port,
                },
                weight: endpoint.weight,
            })
            .collect(),
        #[allow(unreachable_patterns)]
        _ => vec![endpoint.clone()],
    }
}

#[tokio::test]
async fn test_upstream_peer_builder_canonical_sni() {
    use pavis_core::{CanonicalSni, Hostname, SniName, TlsPolicy, TlsVerify};
    use pingora::prelude::HttpPeer;

    let mut config = minimal_config("test");
    config.upstreams[0].tls = TlsPolicy::Enabled {
        verify: TlsVerify::Full,
        sni: SniName::Auto,
        canonical_sni: CanonicalSni::Enabled {
            name: Hostname("canonical.example.com".to_string()),
        },
        reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
        cert: pavis_core::ClientCert::Disabled,
        ca: pavis_core::UpstreamCa::System,
    };

    let validated = pavis_core::validate_runtime(config).expect("validation");
    let manager = Manager::new(&validated.upstreams).expect("manager");
    materialize_test_upstreams(&manager, &validated.upstreams);
    let state = RuntimeState::with_components(
        validated,
        Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
        manager,
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
    ctx.upstream_name = Some(UpstreamName("backend".to_string()));

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let peer: Box<HttpPeer> = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert_eq!(peer.sni, "canonical.example.com");
}

#[tokio::test]
async fn test_upstream_peer_builder_sni_disabled() {
    use pavis_core::{CanonicalSni, SniName, TlsPolicy, TlsVerify};
    use pingora::prelude::HttpPeer;

    let mut config = minimal_config("test");
    config.upstreams[0].tls = TlsPolicy::Enabled {
        verify: TlsVerify::Disabled,
        sni: SniName::Disabled,
        canonical_sni: CanonicalSni::Disabled,
        reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
        cert: pavis_core::ClientCert::Disabled,
        ca: pavis_core::UpstreamCa::System,
    };

    let validated = pavis_core::validate_runtime(config).expect("validation");
    let manager = Manager::new(&validated.upstreams).expect("manager");
    materialize_test_upstreams(&manager, &validated.upstreams);
    let state = RuntimeState::with_components(
        validated,
        Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
        manager,
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
    ctx.upstream_name = Some(UpstreamName("backend".to_string()));

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let peer: Box<HttpPeer> = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    assert!(peer.sni.is_empty());
}

#[tokio::test]
async fn test_upstream_peer_builder_sni_reuse() {
    use pavis_core::{CanonicalSni, Hostname, ReuseAcrossSni, SniName, TlsPolicy, TlsVerify};
    use pingora::prelude::HttpPeer;

    let mut config = minimal_config("test");
    config.upstreams[0].tls = TlsPolicy::Enabled {
        verify: TlsVerify::Full,
        sni: SniName::Name(Hostname("explicit.com".to_string())),
        canonical_sni: CanonicalSni::Disabled,
        reuse_across_sni: ReuseAcrossSni::Enabled,
        cert: pavis_core::ClientCert::Disabled,
        ca: pavis_core::UpstreamCa::System,
    };

    let validated = pavis_core::validate_runtime(config).expect("validation");
    let manager = Manager::new(&validated.upstreams).expect("manager");
    materialize_test_upstreams(&manager, &validated.upstreams);
    let state = RuntimeState::with_components(
        validated,
        Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
        manager,
    );
    let proxy = Proxy {
        state: Arc::new(RuntimeStateHandle::new(state)),
        telemetry: test_telemetry(),
    };

    let mut ctx = proxy.new_ctx();
    pin_runtime_state(&mut ctx, &proxy);
    ctx.upstream_name = Some(UpstreamName("backend".to_string()));

    let (mut session, _client) = session_for_request(b"GET / HTTP/1.1\r\n\r\n").await;
    let peer: Box<HttpPeer> = proxy.upstream_peer(&mut session, &mut ctx).await.unwrap();
    // Should still work, just exercises the SNI reuse branch
    assert!(!peer.sni.is_empty());
}
