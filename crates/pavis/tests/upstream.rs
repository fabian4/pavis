mod common;

use common::base_config;
use pavis::upstream::Manager;
use pavis_core::{
    ConnectionPoolConfig, DiscoveryType, Endpoint, EndpointAddress, HttpVersion, LoadBalancer,
    Upstream, UpstreamTlsConfig,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

#[test]
fn test_upstream_load_balancer_round_robin() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "backend-rr".to_string(),
        discovery_type: DiscoveryType::Static,
        load_balancer: LoadBalancer::RoundRobin,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![
            Endpoint {
                address: EndpointAddress::Ip(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                    80,
                )),
                weight: 1,
            },
            Endpoint {
                address: EndpointAddress::Ip(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
                    80,
                )),
                weight: 1,
            },
        ],
    });

    let manager = Manager::new(&config.upstreams);
    let cluster = manager.get("backend-rr").expect("Cluster not found");

    // Round robin should alternate
    let ep1 = cluster.select_endpoint().unwrap();
    let ep2 = cluster.select_endpoint().unwrap();
    let ep3 = cluster.select_endpoint().unwrap();

    let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

    match ep1.address {
        EndpointAddress::Ip(addr) => assert_eq!(addr.ip(), ip1),
        _ => panic!("expected ip"),
    }
    match ep2.address {
        EndpointAddress::Ip(addr) => assert_eq!(addr.ip(), ip2),
        _ => panic!("expected ip"),
    }
    match ep3.address {
        EndpointAddress::Ip(addr) => assert_eq!(addr.ip(), ip1),
        _ => panic!("expected ip"),
    }
}

#[test]
fn test_upstream_empty_endpoints() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "empty-upstream".to_string(),
        discovery_type: DiscoveryType::Static,
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![],
    });

    let manager = Manager::new(&config.upstreams);
    let cluster = manager.get("empty-upstream").expect("Cluster not found");
    assert!(cluster.select_endpoint().is_none());
}

#[test]
fn test_upstream_tls_config() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "backend-secure".to_string(),
        discovery_type: DiscoveryType::Static,
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: Some(UpstreamTlsConfig {
            enabled: true,
            verify_hostname: false,
            verify_cert: false,
            sni: Some("secure.internal".to_string()),
        }),
        endpoints: vec![Endpoint {
            address: EndpointAddress::Ip(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
                443,
            )),
            weight: 1,
        }],
    });

    let upstream = &config.upstreams[0];

    assert!(upstream.tls.is_some());
    let tls = upstream.tls.as_ref().unwrap();
    assert!(tls.enabled);
    assert_eq!(tls.verify_hostname, false);
    assert_eq!(tls.verify_cert, false);
    assert_eq!(tls.sni, Some("secure.internal".to_string()));
}
