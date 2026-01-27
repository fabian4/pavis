mod common;

use common::*;
use pavis::proxy::service::test_exports::{
    Proxy, endpoint_host_for_sni, generate_request_id, request_id_timestamp, resolve_endpoint_addr,
    resolve_route_timeout, resolve_sni, reuse_key_hash,
};
use pavis::state::{RuntimeState, RuntimeStateHandle};
use pavis::upstream::Manager;
use pavis_core::{
    Discovery, Duration, Endpoint, EndpointAddr, Hostname, Port, SniName, Timeout, UpstreamBuilder,
    UpstreamId, UpstreamName, Weight,
};
use pingora::prelude::ProxyHttp;
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;
use std::time::Instant;

#[test]
fn resolve_route_timeout_maps_enabled() {
    let timeout = Timeout::Enabled(Duration(NonZeroU32::new(150).unwrap()));
    assert_eq!(
        resolve_route_timeout(timeout),
        Some(std::time::Duration::from_millis(150))
    );
}

#[test]
fn new_ctx_defaults_are_empty() {
    let manager = Manager::new(&[]).expect("manager");
    let state = RuntimeState {
        config: RuntimeState::default().config,
        router: Arc::new(pavis::router::Router::new(vec![]).expect("empty routes")),
        upstream_manager: manager,
        config_version: None,
    };
    let state_handle = Arc::new(RuntimeStateHandle::new(state));
    let proxy = Proxy {
        state: state_handle,
        telemetry: test_telemetry(),
    };

    let before = Instant::now();
    let ctx = proxy.new_ctx();
    assert!(ctx.upstream_name.is_none());
    assert!(matches!(
        *ctx.request_headers,
        pavis_core::HeadersPolicy::Disabled
    ));
    assert!(matches!(
        *ctx.response_headers,
        pavis_core::HeadersPolicy::Disabled
    ));
    assert!(ctx.sni_override.is_none());
    assert!(ctx.start_time >= before);
}

#[test]
fn test_resolve_sni() {
    let auto = SniName::Auto;
    let explicit = SniName::Name(Hostname("explicit.com".to_string()));
    let disabled = SniName::Disabled;

    let authority = Hostname("auth.com".to_string());
    let endpoint = Hostname("end.com".to_string());

    assert_eq!(
        resolve_sni(&auto, Some(&authority), Some(&endpoint)),
        Some(authority.clone())
    );
    assert_eq!(
        resolve_sni(&auto, None, Some(&endpoint)),
        Some(endpoint.clone())
    );
    assert_eq!(
        resolve_sni(&explicit, Some(&authority), Some(&endpoint)),
        Some(Hostname("explicit.com".to_string()))
    );
    assert_eq!(
        resolve_sni(&disabled, Some(&authority), Some(&endpoint)),
        None
    );
}

#[test]
fn test_endpoint_host_for_sni() {
    let mut upstream_cfg = UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(1).unwrap()))
        .name(UpstreamName("test".to_string()))
        .discovery(Discovery::Logical)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Dns {
                host: Hostname("h1.com".to_string()),
                port: Port(NonZeroU16::new(80).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .unwrap();

    let ep = &upstream_cfg.endpoints[0];
    assert_eq!(
        endpoint_host_for_sni(&upstream_cfg, ep),
        Some(Hostname("h1.com".to_string()))
    );

    upstream_cfg.endpoints.push(Endpoint {
        address: EndpointAddr::Dns {
            host: Hostname("h2.com".to_string()),
            port: Port(NonZeroU16::new(80).unwrap()),
        },
        weight: Weight(NonZeroU16::new(1).unwrap()),
    });
    // For DNS endpoint, it returns its own host regardless of others
    assert_eq!(
        endpoint_host_for_sni(&upstream_cfg, &upstream_cfg.endpoints[0]),
        Some(Hostname("h1.com".to_string()))
    );

    // For IP endpoint, it checks for consistency among all DNS endpoints
    let ip_ep = Endpoint {
        address: EndpointAddr::Ip {
            address: "127.0.0.1".parse().unwrap(),
            port: Port(NonZeroU16::new(80).unwrap()),
        },
        weight: Weight(NonZeroU16::new(1).unwrap()),
    };
    assert_eq!(endpoint_host_for_sni(&upstream_cfg, &ip_ep), None);
}

#[test]
fn test_resolve_endpoint_addr_ip() {
    let ep = Endpoint {
        address: EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: Port(NonZeroU16::new(8080).unwrap()),
        },
        weight: Weight(NonZeroU16::new(1).unwrap()),
    };
    let addr = resolve_endpoint_addr(&ep).unwrap();
    assert_eq!(addr.to_string(), "127.0.0.1:8080");
}

#[test]
fn request_id_timestamp_handles_pre_epoch_clock() {
    let before_epoch = std::time::UNIX_EPOCH - std::time::Duration::from_secs(1);
    assert_eq!(request_id_timestamp(before_epoch), 0);
}

#[test]
fn request_id_timestamp_computes_nanos() {
    let now = std::time::UNIX_EPOCH + std::time::Duration::from_nanos(12345);
    assert_eq!(request_id_timestamp(now), 12345);
}

#[test]
fn generate_request_id_produces_bounded_ascii() {
    let id = generate_request_id();
    let s = id.as_str();
    assert!(s.starts_with("req-"));
    assert!(s.len() > 10);
    assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
}

#[test]
fn reuse_key_hash_changes_with_tls_inputs() {
    let addr = "127.0.0.1:8080".parse().unwrap();
    let sni1 = "a.com";
    let sni2 = "b.com";

    let h1 = reuse_key_hash(&addr, sni1, None, None);
    let h2 = reuse_key_hash(&addr, sni2, None, None);
    assert_ne!(h1, h2);

    let h3 = reuse_key_hash(&addr, sni1, Some(pavis_core::TlsVerify::Full), None);
    assert_ne!(h1, h3);
}
