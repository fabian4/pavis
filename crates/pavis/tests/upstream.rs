mod common;

use common::base_config;
use pavis::upstream::Manager;
use pavis_core::{
    ConnectTimeout, ConnectionLimit, Duration, Endpoint, EndpointAddr, HttpVersion, IdleTimeout,
    LoadBalancer, Pool, Port, TlsPolicy, TlsVerify, Upstream, UpstreamBuilder, UpstreamId,
    UpstreamName, Weight,
};
use std::net::{IpAddr, Ipv4Addr};
use std::num::{NonZeroU16, NonZeroU32};

fn upstream(name: &str, id: u16, lb: LoadBalancer, port: u16, tls: TlsPolicy) -> Upstream {
    UpstreamBuilder::new()
        .id(UpstreamId(NonZeroU16::new(id).unwrap()))
        .name(UpstreamName(name.to_string()))
        .discovery(pavis_core::Discovery::Static)
        .balancer(lb)
        .protocol(HttpVersion::H1)
        .pool(Pool {
            idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
            connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
            max: ConnectionLimit::Unlimited,
        })
        .tls(tls)
        .add_endpoint(Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::new(10, 0, 0, id as u8)),
                port: Port(NonZeroU16::new(port).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        })
        .build()
        .expect("upstream")
}

#[test]
fn test_upstream_load_balancer_round_robin() {
    let mut config = base_config();
    config.upstreams.push(upstream(
        "backend-rr",
        1,
        LoadBalancer::RoundRobin,
        80,
        TlsPolicy::Disabled,
    ));
    config.upstreams[0].endpoints.push(Endpoint {
        address: EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            port: Port(NonZeroU16::new(80).unwrap()),
        },
        weight: Weight(NonZeroU16::new(1).unwrap()),
    });

    let manager = Manager::new(&config.upstreams).expect("manager");
    let cluster = manager.get("backend-rr").expect("Cluster not found");

    let ep1 = cluster.select_endpoint().unwrap();
    let ep2 = cluster.select_endpoint().unwrap();
    let ep3 = cluster.select_endpoint().unwrap();

    let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

    match ep1.address {
        EndpointAddr::Ip { address, .. } => assert_eq!(address, ip1),
        _ => panic!("expected ip"),
    }
    match ep2.address {
        EndpointAddr::Ip { address, .. } => assert_eq!(address, ip2),
        _ => panic!("expected ip"),
    }
    match ep3.address {
        EndpointAddr::Ip { address, .. } => assert_eq!(address, ip1),
        _ => panic!("expected ip"),
    }
}

#[test]
fn test_upstream_empty_endpoints() {
    let mut config = base_config();
    config.upstreams.push(
        UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(2).unwrap()))
            .name(UpstreamName("empty-upstream".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(60_000).unwrap())),
                connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(5_000).unwrap())),
                max: ConnectionLimit::Unlimited,
            })
            .tls(TlsPolicy::Disabled)
            .build()
            .expect("upstream"),
    );

    let manager = Manager::new(&config.upstreams).expect("manager");
    let cluster = manager.get("empty-upstream").expect("Cluster not found");
    assert!(cluster.select_endpoint().is_none());
}

#[test]
fn test_upstream_tls_config() {
    let mut config = base_config();
    config.upstreams.push(upstream(
        "backend-secure",
        1,
        LoadBalancer::Random,
        443,
        TlsPolicy::Enabled {
            verify: TlsVerify::Disabled,
            sni: pavis_core::SniName::Name(pavis_core::Hostname("secure.internal".to_string())),
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        },
    ));

    let upstream = &config.upstreams[0];

    match upstream.tls {
        TlsPolicy::Enabled { verify, .. } => {
            assert_eq!(verify, TlsVerify::Disabled);
        }
        TlsPolicy::Disabled => panic!("tls not enabled"),
        _ => panic!("unknown tls policy"),
    }
}
