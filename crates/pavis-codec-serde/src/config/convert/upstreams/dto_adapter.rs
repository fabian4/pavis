use crate::config::types::{
    CircuitBreaker, ClientCertChainMode, ClientCertConfig, ConnectionPoolConfig, Endpoint,
    HealthCheck, OutlierDetection, SniMode, Upstream, UpstreamTlsConfig,
};
use anyhow::{Result, anyhow};
use pavis_core::{
    ActiveHealthCheck, CanonicalSni, CircuitBreakerPolicy, ClientCert, ClientCertChain,
    ConnectTimeout, EndpointAddr, IdleTimeout, OutlierDetectionPolicy, ReuseAcrossSni, SniName,
    TlsVerify, UpstreamCa,
};

pub fn from_runtime(upstreams: Vec<pavis_core::Upstream>) -> Result<Vec<Upstream>> {
    let mut serde_upstreams = Vec::new();

    for u in upstreams {
        let mut endpoints = Vec::new();
        for e in u.endpoints {
            let (address, port) = match e.address {
                EndpointAddr::Ip { address, port } => (address.to_string(), port.0.get()),
                EndpointAddr::Dns { host, port } => (host.0, port.0.get()),
                #[allow(unreachable_patterns)]
                _ => {
                    return Err(anyhow!("unknown endpoint address variant"));
                }
            };
            endpoints.push(Endpoint {
                address,
                port,
                weight: Some(e.weight.0.get() as u32),
            });
        }

        let pool = Some(ConnectionPoolConfig {
            idle: Some(std::time::Duration::from_millis(idle_timeout_ms(
                &u.pool.idle,
            ))),
            connect: Some(std::time::Duration::from_millis(connect_timeout_ms(
                &u.pool.connect,
            ))),
            max: Some(u.pool.max.0.get() as i64),
            queue_capacity: Some(u.pool.queue.capacity as i64),
            queue_timeout_ms: Some(u.pool.queue.timeout_ms as i64),
            tcp_keepalive: u
                .pool
                .tcp_keepalive
                .map(|d| std::time::Duration::from_millis(d.0.get() as u64)),
            tcp_nodelay: u.pool.tcp_nodelay,
            recv_buffer_size: u.pool.recv_buffer_size,
        });

        let tls = match u.tls {
            pavis_core::TlsPolicy::Disabled => None,
            pavis_core::TlsPolicy::Enabled {
                verify,
                sni,
                canonical_sni,
                reuse_across_sni,
                cert,
                ca,
            } => {
                let (verify_cert, verify_hostname) = match verify {
                    TlsVerify::Disabled => (false, false),
                    TlsVerify::CaOnly => (true, false),
                    TlsVerify::Full => (true, true),
                    #[allow(unreachable_patterns)]
                    _ => (true, true),
                };
                let (sni, sni_mode) = match sni {
                    SniName::Auto => (None, Some(SniMode::Auto)),
                    SniName::Name(name) => (Some(name.0), Some(SniMode::Name)),
                    SniName::Disabled => (None, Some(SniMode::Disabled)),
                    #[allow(unreachable_patterns)]
                    _ => (None, Some(SniMode::Auto)),
                };
                let ca_bundle_path = match ca {
                    UpstreamCa::System => None,
                    UpstreamCa::File { path } => Some(path.0),
                    #[allow(unreachable_patterns)]
                    _ => None,
                };
                let cert_config = match cert {
                    ClientCert::Disabled => None,
                    ClientCert::Enabled {
                        cert_path,
                        key_path,
                        chain,
                    } => {
                        let (chain_mode, chain_path) = match chain {
                            ClientCertChain::None => (Some(ClientCertChainMode::None), None),
                            ClientCertChain::Embedded => {
                                (Some(ClientCertChainMode::Embedded), None)
                            }
                            ClientCertChain::File { path } => {
                                (Some(ClientCertChainMode::File), Some(path.0))
                            }
                            #[allow(unreachable_patterns)]
                            _ => (Some(ClientCertChainMode::None), None),
                        };
                        Some(ClientCertConfig {
                            cert_path: cert_path.0,
                            key_path: key_path.0,
                            chain_path,
                            chain_mode,
                        })
                    }
                    #[allow(unreachable_patterns)]
                    _ => None,
                };
                let canonical_sni = match canonical_sni {
                    CanonicalSni::Disabled => None,
                    CanonicalSni::Enabled { name } => Some(name.0),
                    #[allow(unreachable_patterns)]
                    _ => None,
                };
                let reuse_across_sni = match reuse_across_sni {
                    ReuseAcrossSni::Enabled => Some(true),
                    ReuseAcrossSni::Disabled => None,
                    #[allow(unreachable_patterns)]
                    _ => None,
                };
                Some(UpstreamTlsConfig {
                    enabled: Some(true),
                    verify_hostname: Some(verify_hostname),
                    verify_cert: Some(verify_cert),
                    sni,
                    sni_mode,
                    canonical_sni,
                    reuse_across_sni,
                    ca_bundle_path,
                    cert: cert_config,
                })
            }
            #[allow(unreachable_patterns)]
            _ => None,
        };

        serde_upstreams.push(Upstream {
            id: Some(u.id.0.get()),
            name: u.name.0,
            discovery: Some(u.discovery),
            balancer: Some(u.balancer),
            protocol: Some(u.protocol),
            pool,
            tls,
            circuit_breaker: match u.circuit_breaker {
                CircuitBreakerPolicy::Disabled => None,
                CircuitBreakerPolicy::Enabled {
                    max_connections,
                    max_pending_requests,
                } => Some(CircuitBreaker {
                    max_connections: max_connections.0.get() as usize,
                    max_pending_requests: max_pending_requests.0.get() as usize,
                    max_retries: None,
                }),
                #[allow(unreachable_patterns)]
                _ => None,
            },
            outlier_detection: match u.outlier_detection {
                OutlierDetectionPolicy::Disabled => None,
                OutlierDetectionPolicy::Enabled {
                    consecutive_errors,
                    eject_duration,
                } => Some(OutlierDetection {
                    consecutive_errors: consecutive_errors.0.get() as usize,
                    eject_duration: std::time::Duration::from_millis(eject_duration.0.get() as u64),
                }),
                #[allow(unreachable_patterns)]
                _ => None,
            },
            health_check: match u.health_check {
                ActiveHealthCheck::Disabled => None,
                ActiveHealthCheck::Enabled {
                    path,
                    interval,
                    timeout,
                } => Some(HealthCheck {
                    path: path.0,
                    interval: std::time::Duration::from_millis(interval.0.get() as u64),
                    timeout: Some(std::time::Duration::from_millis(timeout.0.get() as u64)),
                    healthy_threshold: 1,
                    unhealthy_threshold: 1,
                }),
                #[allow(unreachable_patterns)]
                _ => None,
            },
            endpoints,
        });
    }

    Ok(serde_upstreams)
}

fn idle_timeout_ms(timeout: &IdleTimeout) -> u64 {
    match timeout {
        IdleTimeout::Disabled => 0,
        IdleTimeout::Enabled(d) => d.0.get() as u64,
        _ => 0,
    }
}

fn connect_timeout_ms(timeout: &ConnectTimeout) -> u64 {
    match timeout {
        ConnectTimeout::Disabled => 0,
        ConnectTimeout::Enabled(d) => d.0.get() as u64,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        CanonicalSni, ClientCert, ClientCertChain, ConnectTimeout, ConnectionLimit,
        ConsecutiveErrors, Discovery, Duration, Endpoint as RuntimeEndpoint, EndpointAddr,
        Hostname, HttpVersion, IdleTimeout, LoadBalancer, MaxConnections, MaxPendingRequests,
        OutlierDetectionPolicy, Path, Pool, PoolQueue, Port, ReuseAcrossSni, SniName, TlsPolicy,
        TlsVerify, UpstreamBuilder, UpstreamCa, UpstreamId, UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::{NonZeroU16, NonZeroU32};

    #[test]
    fn test_from_runtime_empty() {
        let res = from_runtime(vec![]).unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn test_from_runtime_tls_variants() {
        // Test CaOnly verify and Auto SNI
        let u1 = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("u1".into()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::CaOnly,
                sni: SniName::Auto,
                canonical_sni: CanonicalSni::Disabled,
                reuse_across_sni: ReuseAcrossSni::Disabled,
                cert: ClientCert::Disabled,
                ca: UpstreamCa::System,
            })
            .add_endpoint(RuntimeEndpoint {
                address: EndpointAddr::Ip {
                    address: "127.0.0.1".parse().unwrap(),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        // Test Disabled verify and Disabled SNI
        let u2 = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(2).unwrap()))
            .name(UpstreamName("u2".into()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::Disabled,
                sni: SniName::Disabled,
                canonical_sni: CanonicalSni::Disabled,
                reuse_across_sni: ReuseAcrossSni::Disabled,
                cert: ClientCert::Disabled,
                ca: UpstreamCa::System,
            })
            .add_endpoint(RuntimeEndpoint {
                address: EndpointAddr::Ip {
                    address: "127.0.0.1".parse().unwrap(),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        let res = from_runtime(vec![u1, u2]).unwrap();

        let dto1 = &res[0].tls.as_ref().unwrap();
        assert_eq!(dto1.verify_cert, Some(true));
        assert_eq!(dto1.verify_hostname, Some(false));
        assert_eq!(dto1.sni_mode, Some(SniMode::Auto));

        let dto2 = &res[1].tls.as_ref().unwrap();
        assert_eq!(dto2.verify_cert, Some(false));
        assert_eq!(dto2.verify_hostname, Some(false));
        assert_eq!(dto2.sni_mode, Some(SniMode::Disabled));
    }

    #[test]
    fn test_from_runtime_client_cert_embedded() {
        let u = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("u".into()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::Disabled,
                sni: SniName::Disabled,
                canonical_sni: CanonicalSni::Disabled,
                reuse_across_sni: ReuseAcrossSni::Disabled,
                cert: ClientCert::Enabled {
                    cert_path: Path("/c".into()),
                    key_path: Path("/k".into()),
                    chain: ClientCertChain::Embedded,
                },
                ca: UpstreamCa::System,
            })
            .add_endpoint(RuntimeEndpoint {
                address: EndpointAddr::Ip {
                    address: "127.0.0.1".parse().unwrap(),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        let res = from_runtime(vec![u]).unwrap();
        let cert = res[0].tls.as_ref().unwrap().cert.as_ref().unwrap();
        assert_eq!(cert.chain_mode, Some(ClientCertChainMode::Embedded));
        assert!(cert.chain_path.is_none());
    }

    #[test]
    fn test_from_runtime_full() {
        let u = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H2)
            .pool(Pool {
                idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(1000).unwrap())),
                connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(2000).unwrap())),
                max: ConnectionLimit(NonZeroU32::new(10).unwrap()),
                queue: PoolQueue {
                    capacity: 100,
                    timeout_ms: 5000,
                },
                tcp_keepalive: Some(Duration(NonZeroU32::new(60000).unwrap())),
                tcp_nodelay: Some(true),
                recv_buffer_size: Some(65536),
            })
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::Full,
                sni: SniName::Name(Hostname("example.com".into())),
                canonical_sni: CanonicalSni::Enabled {
                    name: Hostname("canonical.com".into()),
                },
                reuse_across_sni: ReuseAcrossSni::Enabled,
                cert: ClientCert::Enabled {
                    cert_path: Path("/cert".into()),
                    key_path: Path("/key".into()),
                    chain: ClientCertChain::File {
                        path: Path("/chain".into()),
                    },
                },
                ca: UpstreamCa::File {
                    path: Path("/ca".into()),
                },
            })
            .circuit_breaker(CircuitBreakerPolicy::Enabled {
                max_connections: MaxConnections(NonZeroU32::new(5).unwrap()),
                max_pending_requests: MaxPendingRequests(NonZeroU32::new(5).unwrap()),
            })
            .outlier_detection(OutlierDetectionPolicy::Enabled {
                consecutive_errors: ConsecutiveErrors(NonZeroU32::new(3).unwrap()),
                eject_duration: Duration(NonZeroU32::new(30000).unwrap()),
            })
            .health_check(ActiveHealthCheck::Enabled {
                path: Path("/health".into()),
                interval: Duration(NonZeroU32::new(5000).unwrap()),
                timeout: Duration(NonZeroU32::new(1000).unwrap()),
            })
            .add_endpoint(pavis_core::Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .add_endpoint(pavis_core::Endpoint {
                address: EndpointAddr::Dns {
                    host: Hostname("backend".into()),
                    port: Port(NonZeroU16::new(8081).unwrap()),
                },
                weight: Weight(NonZeroU16::new(2).unwrap()),
            })
            .build()
            .unwrap();

        let res = from_runtime(vec![u]).unwrap();
        assert_eq!(res.len(), 1);
        let dto = &res[0];
        assert_eq!(dto.id, Some(1));
        assert_eq!(dto.name, "test");
        assert_eq!(dto.discovery, Some(Discovery::Static));
        assert_eq!(dto.balancer, Some(LoadBalancer::RoundRobin));
        assert_eq!(dto.protocol, Some(HttpVersion::H2));

        let p = dto.pool.as_ref().unwrap();
        assert_eq!(p.max, Some(10));
        assert_eq!(p.queue_capacity, Some(100));
        assert_eq!(p.tcp_nodelay, Some(true));

        let t = dto.tls.as_ref().unwrap();
        assert_eq!(t.verify_hostname, Some(true));
        assert_eq!(t.sni, Some("example.com".to_string()));
        assert_eq!(t.canonical_sni, Some("canonical.com".to_string()));
        assert_eq!(t.reuse_across_sni, Some(true));
        assert_eq!(t.ca_bundle_path, Some("/ca".to_string()));

        let c = t.cert.as_ref().unwrap();
        assert_eq!(c.cert_path, "/cert");
        assert_eq!(c.chain_path, Some("/chain".to_string()));
        assert_eq!(c.chain_mode, Some(ClientCertChainMode::File));

        let cb = dto.circuit_breaker.as_ref().unwrap();
        assert_eq!(cb.max_connections, 5);

        let od = dto.outlier_detection.as_ref().unwrap();
        assert_eq!(od.consecutive_errors, 3);

        let hc = dto.health_check.as_ref().unwrap();
        assert_eq!(hc.path, "/health");

        assert_eq!(dto.endpoints.len(), 2);
        assert_eq!(dto.endpoints[0].address, "127.0.0.1");
        assert_eq!(dto.endpoints[1].address, "backend");
    }

    #[test]
    fn test_from_runtime_minimal_tls_disabled() {
        let u = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("min".to_string()))
            .discovery(Discovery::Logical)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .tls(TlsPolicy::Disabled)
            .add_endpoint(pavis_core::Endpoint {
                address: EndpointAddr::Dns {
                    host: Hostname("h".into()),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        let res = from_runtime(vec![u]).unwrap();
        let dto = &res[0];
        assert!(dto.tls.is_none());
        assert_eq!(dto.discovery, Some(Discovery::Logical));
    }
}
