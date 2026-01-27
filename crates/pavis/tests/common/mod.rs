#![allow(dead_code)]
use pavis::proxy::context::RouterContext;
use pavis::proxy::service::test_exports::Proxy;
use pavis::state::RuntimeStateHandle;
use pavis::telemetry::Telemetry;
use pavis_core::{
    AccessLogPolicy, ClientCert, ClientCertChain, ConnectTimeout, ConnectionLimit, Discovery,
    Duration, Endpoint, EndpointAddr, HeadersPolicy, HttpVersion, IdleTimeout, ListenerBuilder,
    ListenerName, LoadBalancer, Metrics, Path, PathMatch, Port, RuntimeConfig as Config,
    RuntimeConfigBuilder, ServiceName, SniName, Telemetry as RuntimeTelemetry, TlsPolicy, Upstream,
    UpstreamBuilder, UpstreamCa, UpstreamId, UpstreamName, Weight, WorkerCount,
};
pub use pingora::prelude::Session;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroU16;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub fn base_config() -> Config {
    let listener = ListenerBuilder::new()
        .name(ListenerName("default".to_string()))
        .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080))
        .workers(WorkerCount::Auto)
        .tls(pavis_core::TlsConfig::Disabled)
        .build()
        .expect("listener");

    RuntimeConfigBuilder::new()
        .telemetry(RuntimeTelemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName("pavis".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .build()
        .expect("config")
}

pub fn test_telemetry() -> Arc<Telemetry> {
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

pub fn pin_runtime_state(ctx: &mut RouterContext, proxy: &Proxy) {
    ctx.runtime_state = Some(proxy.state.load());
}

pub async fn session_for_request(request: &[u8]) -> (Session, tokio::io::DuplexStream) {
    let (mut client, server) = tokio::io::duplex(1024);
    client.write_all(request).await.expect("write request");
    let mut session = Session::new_h1(Box::new(server));
    session.read_request().await.expect("read request");
    (session, client)
}

pub fn upstream(name: &str, id: u16, port: u16) -> Upstream {
    UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(id).unwrap()))
        .name(UpstreamName(name.to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .pool(pavis_core::Pool {
            idle: IdleTimeout::Enabled(Duration(std::num::NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(std::num::NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit(std::num::NonZeroU32::new(128).unwrap()),
            ..pavis_core::Pool::default()
        })
        .tls(TlsPolicy::Disabled)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(port).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream")
}

pub fn write_pem(path: &StdPath, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write pem");
}

pub fn build_self_signed_cert() -> (String, String) {
    let mut params = rcgen::CertificateParams::new(vec!["client".to_string()]).unwrap();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "client");
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    (key_pair.serialize_pem(), cert.pem())
}

pub fn mtls_upstream(
    name: &str,
    id: u16,
    port: u16,
    cert_path: PathBuf,
    key_path: PathBuf,
) -> Upstream {
    UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(id).unwrap()))
        .name(UpstreamName(name.to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::Random)
        .protocol(HttpVersion::H1)
        .pool(pavis_core::Pool {
            idle: IdleTimeout::Enabled(Duration(std::num::NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(std::num::NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit(std::num::NonZeroU32::new(128).unwrap()),
            ..pavis_core::Pool::default()
        })
        .tls(TlsPolicy::Enabled {
            verify: pavis_core::TlsVerify::Disabled,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: ClientCert::Enabled {
                cert_path: pavis_core::Path(cert_path.to_string_lossy().to_string()),
                key_path: pavis_core::Path(key_path.to_string_lossy().to_string()),
                chain: ClientCertChain::None,
            },
            ca: UpstreamCa::System,
        })
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(port).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream")
}

pub fn minimal_config(name: &str) -> Config {
    let listener = ListenerBuilder::new()
        .name(ListenerName("default".to_string()))
        .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0))
        .workers(WorkerCount::Auto)
        .tls(pavis_core::TlsConfig::Disabled)
        .build()
        .expect("listener");

    let upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("backend".to_string()))
        .discovery(Discovery::Static)
        .balancer(LoadBalancer::RoundRobin)
        .protocol(HttpVersion::H1)
        .pool(pavis_core::Pool {
            idle: IdleTimeout::Enabled(Duration(std::num::NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(std::num::NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit(std::num::NonZeroU32::new(128).unwrap()),
            ..pavis_core::Pool::default()
        })
        .tls(TlsPolicy::Disabled)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8080).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream");

    RuntimeConfigBuilder::new()
        .telemetry(RuntimeTelemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName(name.to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: pavis_core::TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .add_upstream(upstream_cfg)
        .add_route(pavis_core::VirtualHost {
            host: pavis_core::Host("*".to_string()),
            paths: vec![pavis_core::Route {
                matcher: pavis_core::RouteMatcher {
                    path: PathMatch::Prefix {
                        path: Path("/".to_string()),
                    },
                    method: pavis_core::MethodPredicate::Any,
                    headers: pavis_core::HeaderPredicates::None,
                },
                timeout: pavis_core::Timeout::Disabled,
                retry: pavis_core::RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled.into(),
                response_headers: HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Any,
                rewrite: pavis_core::Rewrite {
                    path: pavis_core::RewritePath::Disabled,
                    host: pavis_core::RewriteHost::Disabled,
                },
                action: pavis_core::RouteAction::Forward(vec![pavis_core::Destination {
                    upstream: UpstreamName("backend".to_string()),
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                }]),
            }],
        })
        .build()
        .expect("config")
}

pub fn config_with_upstream(service_name: &str, upstream_name: &str) -> Config {
    let mut config = minimal_config(service_name);
    config.upstreams[0].name = UpstreamName(upstream_name.to_string());
    if let pavis_core::RouteAction::Forward(destinations) = &mut config.routes[0].paths[0].action {
        destinations[0].upstream = UpstreamName(upstream_name.to_string());
    }
    config
}

pub fn write_pvs(path: &StdPath, name: &str) -> Vec<u8> {
    let config = minimal_config(name);
    pavis_pvs::write(path, &config).expect("write");
    std::fs::read(path).expect("read")
}

pub fn pvs_bytes(name: &str) -> Vec<u8> {
    let config = minimal_config(name);
    pavis_pvs::encode(&config).expect("encode")
}

pub fn etag_for_bytes(bytes: &[u8]) -> String {
    let digest = pavis_pvs::compute_checksum(bytes);
    let mut out = String::with_capacity(digest.len() * 2 + "sha256:".len());
    out.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn make_agent(
    base: String,
    lkg_path: PathBuf,
    state: Arc<RuntimeStateHandle>,
) -> Arc<pavis::agent::ConfigAgent> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client");
    Arc::new(pavis::agent::test_exports::config_agent_new_for_tests(
        base,
        lkg_path.clone(),
        state,
        client,
        pavis::agent::Backoff::new(
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(30),
            0,
        ),
    ))
}
