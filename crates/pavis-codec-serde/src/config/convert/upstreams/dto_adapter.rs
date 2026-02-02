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
