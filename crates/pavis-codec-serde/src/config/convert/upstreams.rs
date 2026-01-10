use anyhow::{Context, Result};
use std::net::IpAddr;
use std::num::{NonZeroU16, NonZeroU32};

use pavis_core::{ClientCert, ConnectTimeout, Discovery, EndpointAddr, Path, TlsVerify};

use crate::config::types::{ClientCertConfig, Endpoint, Upstream, UpstreamTlsConfig};

pub(super) fn to_runtime(upstreams: Vec<Upstream>) -> Result<Vec<pavis_core::Upstream>> {
    let mut runtime_upstreams = Vec::new();

    for (index, u) in upstreams.into_iter().enumerate() {
        let discovery = u.discovery.unwrap_or_default();
        let balancer = u.balancer.unwrap_or_default();
        let protocol = u.protocol.unwrap_or_default();
        let pool_config = u.pool.unwrap_or_else(default_pool_config);

        let mut endpoints = Vec::new();
        for e in u.endpoints {
            let port = NonZeroU16::new(e.port)
                .ok_or_else(|| anyhow::anyhow!("endpoint port must be > 0"))?;
            let address = match discovery {
                Discovery::Static => {
                    let ip: IpAddr = e.address.parse().with_context(|| {
                        format!(
                            "Invalid endpoint IP '{}' for upstream '{}'",
                            e.address, u.name
                        )
                    })?;
                    EndpointAddr::Ip {
                        address: ip,
                        port: pavis_core::Port(port),
                    }
                }
                Discovery::Logical | Discovery::Strict { .. } => EndpointAddr::Dns {
                    host: pavis_core::Hostname(e.address),
                    port: pavis_core::Port(port),
                },
                _ => return Err(anyhow::anyhow!("unknown discovery variant")),
            };

            let weight = e.weight.unwrap_or(1);
            let weight = u16::try_from(weight).context("endpoint weight exceeds u16::MAX")?;
            let weight = NonZeroU16::new(weight)
                .ok_or_else(|| anyhow::anyhow!("endpoint weight must be > 0"))?;
            endpoints.push(pavis_core::Endpoint {
                address,
                weight: pavis_core::Weight(weight),
            });
        }

        let idle = duration_to_policy(pool_config.idle.unwrap_or_else(default_idle_timeout))?;
        let connect = duration_to_connect(
            pool_config
                .connect
                .unwrap_or_else(default_connection_timeout),
        )?;
        let max = match pool_config.max {
            None | Some(0) => pavis_core::ConnectionLimit::Unlimited,
            Some(value) => {
                let value = NonZeroU32::new(value)
                    .ok_or_else(|| anyhow::anyhow!("pool.max must be > 0"))?;
                pavis_core::ConnectionLimit::Limited(value)
            }
        };

        let pool = pavis_core::Pool { idle, connect, max };

        let tls = match u.tls {
            None => pavis_core::TlsPolicy::Disabled,
            Some(t) => {
                let enabled = t.enabled.unwrap_or(true);
                if !enabled {
                    pavis_core::TlsPolicy::Disabled
                } else {
                    let verify_cert = t.verify_cert.unwrap_or(true);
                    let verify_hostname = t.verify_hostname.unwrap_or(true);
                    let mode = match (verify_cert, verify_hostname) {
                        (false, _) => TlsVerify::Disabled,
                        (true, false) => TlsVerify::Cert,
                        (true, true) => TlsVerify::CertAndHost,
                    };
                    let sni = match t.sni {
                        Some(name) => pavis_core::SniName::Value(pavis_core::Hostname(name)),
                        None => pavis_core::SniName::Auto,
                    };
                    let cert = match t.cert {
                        None => ClientCert::Disabled,
                        Some(cc) => ClientCert::Enabled {
                            cert_path: Path(cc.cert_path),
                            key_path: Path(cc.key_path),
                        },
                    };
                    pavis_core::TlsPolicy::Enabled { mode, sni, cert }
                }
            }
        };

        let id = match u.id {
            Some(id) => {
                NonZeroU16::new(id).ok_or_else(|| anyhow::anyhow!("upstream id must be > 0"))?
            }
            None => NonZeroU16::new((index + 1) as u16)
                .ok_or_else(|| anyhow::anyhow!("upstream id must be > 0"))?,
        };

        runtime_upstreams.push(pavis_core::Upstream {
            id: pavis_core::UpstreamId(id),
            name: pavis_core::UpstreamName(u.name),
            discovery,
            balancer,
            protocol,
            pool,
            tls,
            endpoints,
        });
    }

    Ok(runtime_upstreams)
}

pub(super) fn from_runtime(upstreams: Vec<pavis_core::Upstream>) -> Vec<Upstream> {
    let mut serde_upstreams = Vec::new();

    for u in upstreams {
        let mut endpoints = Vec::new();
        for e in u.endpoints {
            let (address, port) = match e.address {
                EndpointAddr::Ip { address, port } => (address.to_string(), port.0.get()),
                EndpointAddr::Dns { host, port } => (host.0, port.0.get()),
                #[allow(unreachable_patterns)]
                _ => {
                    panic!("unknown endpoint address variant");
                }
            };
            endpoints.push(Endpoint {
                address,
                port,
                weight: Some(e.weight.0.get() as u32),
            });
        }

        let pool = Some(crate::config::types::ConnectionPoolConfig {
            idle: Some(std::time::Duration::from_millis(idle_timeout_ms(
                &u.pool.idle,
            ))),
            connect: Some(std::time::Duration::from_millis(connect_timeout_ms(
                &u.pool.connect,
            ))),
            max: match u.pool.max {
                pavis_core::ConnectionLimit::Unlimited => None,
                pavis_core::ConnectionLimit::Limited(value) => Some(value.get()),
                #[allow(unreachable_patterns)]
                _ => {
                    // Sensible default: treat as unlimited if variant is unknown
                    None
                }
            },
        });

        let tls = match u.tls {
            pavis_core::TlsPolicy::Disabled => None,
            pavis_core::TlsPolicy::Enabled { mode, sni, cert } => {
                let (verify_cert, verify_hostname) = match mode {
                    TlsVerify::Disabled => (false, false),
                    TlsVerify::Cert => (true, false),
                    TlsVerify::CertAndHost => (true, true),
                    #[allow(unreachable_patterns)]
                    _ => {
                        // Sensible default: treat as CertAndHost if variant is unknown
                        (true, true)
                    }
                };
                let sni = match sni {
                    pavis_core::SniName::Auto => None,
                    pavis_core::SniName::Value(name) => Some(name.0),
                    #[allow(unreachable_patterns)]
                    _ => {
                        // Sensible default: treat as Auto if variant is unknown
                        None
                    }
                };
                let cert_config = match cert {
                    ClientCert::Disabled => None,
                    ClientCert::Enabled {
                        cert_path,
                        key_path,
                    } => Some(ClientCertConfig {
                        cert_path: cert_path.0,
                        key_path: key_path.0,
                    }),
                    #[allow(unreachable_patterns)]
                    _ => {
                        // Sensible default: treat as Disabled if variant is unknown
                        None
                    }
                };
                Some(UpstreamTlsConfig {
                    enabled: Some(true),
                    verify_hostname: Some(verify_hostname),
                    verify_cert: Some(verify_cert),
                    sni,
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
            circuit_breaker: None,
            health_check: None,
            endpoints,
        });
    }

    serde_upstreams
}

fn default_pool_config() -> crate::config::types::ConnectionPoolConfig {
    crate::config::types::ConnectionPoolConfig {
        idle: Some(default_idle_timeout()),
        connect: Some(default_connection_timeout()),
        max: None,
    }
}

fn default_idle_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(60)
}

fn default_connection_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(5)
}

fn duration_to_policy(duration: std::time::Duration) -> Result<pavis_core::IdleTimeout> {
    let ms = u32::try_from(duration.as_millis())
        .map_err(|_| anyhow::anyhow!("idle timeout exceeds u32::MAX ms"))?;
    Ok(match NonZeroU32::new(ms) {
        Some(ms) => pavis_core::IdleTimeout::Enabled(pavis_core::Duration(ms)),
        None => pavis_core::IdleTimeout::Disabled,
    })
}

fn duration_to_connect(duration: std::time::Duration) -> Result<ConnectTimeout> {
    let ms = u32::try_from(duration.as_millis())
        .map_err(|_| anyhow::anyhow!("connect timeout exceeds u32::MAX ms"))?;
    Ok(match NonZeroU32::new(ms) {
        Some(ms) => ConnectTimeout::Enabled(pavis_core::Duration(ms)),
        None => ConnectTimeout::Disabled,
    })
}

fn idle_timeout_ms(timeout: &pavis_core::IdleTimeout) -> u64 {
    match timeout {
        pavis_core::IdleTimeout::Disabled => 0,
        pavis_core::IdleTimeout::Enabled(d) => d.0.get() as u64,
        _ => 0,
    }
}

fn connect_timeout_ms(timeout: &pavis_core::ConnectTimeout) -> u64 {
    match timeout {
        pavis_core::ConnectTimeout::Disabled => 0,
        pavis_core::ConnectTimeout::Enabled(d) => d.0.get() as u64,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{ConnectionPoolConfig, Endpoint, Upstream};
    use pavis_core::{
        ConnectTimeout, ConnectionLimit, Discovery, EndpointAddr, HttpVersion, IdleTimeout,
        LoadBalancer, Pool, Port, TlsPolicy, UpstreamId, UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU16;

    #[test]
    fn to_runtime_validates_endpoint_addresses() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "invalid-ip".to_string(),
                port: 80,
                weight: None,
            }],
        }];
        assert!(to_runtime(config).is_err());
    }

    #[test]
    fn to_runtime_defaults() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).unwrap();
        let u = &runtime[0];
        assert_eq!(u.name.0, "test");
        assert!(matches!(u.discovery, Discovery::Static));
        assert!(matches!(u.balancer, LoadBalancer::Random));
        assert!(matches!(u.protocol, HttpVersion::H1));
        assert!(matches!(u.tls, TlsPolicy::Disabled));
        assert!(u.endpoints.is_empty());
    }

    #[test]
    fn to_runtime_pool_defaults() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: None,
            }),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).unwrap();
        let pool = &runtime[0].pool;
        assert!(matches!(pool.idle, IdleTimeout::Enabled(_))); // Default 60s
        assert!(matches!(pool.connect, ConnectTimeout::Enabled(_))); // Default 5s
        assert!(matches!(pool.max, ConnectionLimit::Unlimited));
    }

    #[test]
    fn to_runtime_validates_pool_max() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: Some(0), // becomes Unlimited but let's test explicit > 0
            }),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).unwrap();
        assert!(matches!(
            runtime[0].pool.max,
            pavis_core::ConnectionLimit::Unlimited
        ));
    }

    #[test]
    fn tls_enabled_false_conversion() {
        let config = vec![Upstream {
            id: None,
            name: "test".to_string(),
            discovery: None,
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(false),
                verify_hostname: None,
                verify_cert: None,
                sni: None,
                cert: None,
            }),
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![],
        }];
        let runtime = to_runtime(config).unwrap();
        assert!(matches!(runtime[0].tls, TlsPolicy::Disabled));
    }

    #[test]
    fn tls_verify_modes_conversion() {
        let test_cases = vec![
            ((false, false), TlsVerify::Disabled),
            ((true, false), TlsVerify::Cert),
            ((true, true), TlsVerify::CertAndHost),
        ];

        for ((cert, host), expected_mode) in test_cases {
            let config = vec![Upstream {
                id: None,
                name: "test".to_string(),
                discovery: None,
                balancer: None,
                protocol: None,
                pool: None,
                tls: Some(UpstreamTlsConfig {
                    enabled: Some(true),
                    verify_hostname: Some(host),
                    verify_cert: Some(cert),
                    sni: None,
                    cert: None,
                }),
                circuit_breaker: None,
                health_check: None,
                endpoints: vec![],
            }];
            let runtime = to_runtime(config).unwrap();
            match runtime[0].tls {
                TlsPolicy::Enabled { mode, .. } => assert_eq!(mode, expected_mode),
                _ => panic!("expected tls enabled"),
            }
        }
    }

    #[test]
    fn from_runtime_round_trips() {
        let runtime = pavis_core::Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("test".to_string()),
            discovery: Discovery::Static,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H2,
            pool: Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Disabled,
            endpoints: vec![pavis_core::Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(5).unwrap()),
            }],
        };

        let config = from_runtime(vec![runtime]);
        let u = &config[0];
        assert_eq!(u.name, "test");
        assert!(matches!(u.balancer, Some(LoadBalancer::RoundRobin)));
        assert!(matches!(u.protocol, Some(HttpVersion::H2)));
        assert_eq!(u.endpoints.len(), 1);
        assert_eq!(u.endpoints[0].address, "127.0.0.1");
        assert_eq!(u.endpoints[0].port, 8080);
        assert_eq!(u.endpoints[0].weight, Some(5));
    }

    #[test]
    fn dns_discovery_and_tls_conversion() {
        use crate::config::types::{ClientCertConfig, UpstreamTlsConfig};
        let config = vec![Upstream {
            id: None,
            name: "dns".to_string(),
            discovery: Some(Discovery::Logical),
            balancer: None,
            protocol: None,
            pool: None,
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(true),
                verify_cert: Some(true),
                sni: Some("example.com".to_string()),
                cert: Some(ClientCertConfig {
                    cert_path: "c.pem".to_string(),
                    key_path: "k.pem".to_string(),
                }),
            }),
            circuit_breaker: None,
            health_check: None,
            endpoints: vec![Endpoint {
                address: "example.com".to_string(),
                port: 80,
                weight: None,
            }],
        }];

        let runtime = to_runtime(config).unwrap();
        let u = &runtime[0];
        match u.discovery {
            Discovery::Logical => {}
            _ => panic!("expected logical discovery"),
        }
        match &u.endpoints[0].address {
            EndpointAddr::Dns { host, port } => {
                assert_eq!(host.0, "example.com");
                assert_eq!(port.0.get(), 80);
            }
            _ => panic!("expected dns endpoint"),
        }
        match &u.tls {
            TlsPolicy::Enabled { mode, sni, cert } => {
                assert!(matches!(mode, pavis_core::TlsVerify::CertAndHost));
                match sni {
                    pavis_core::SniName::Value(s) => assert_eq!(s.0, "example.com"),
                    _ => panic!("expected sni value"),
                }
                match cert {
                    pavis_core::ClientCert::Enabled {
                        cert_path,
                        key_path,
                    } => {
                        assert_eq!(cert_path.0, "c.pem");
                        assert_eq!(key_path.0, "k.pem");
                    }
                    _ => panic!("expected client cert"),
                }
            }
            _ => panic!("expected tls enabled"),
        }

        // Round trip
        let serde = from_runtime(runtime);
        let u_serde = &serde[0];
        match u_serde.endpoints[0].address.as_str() {
            "example.com" => {}
            _ => panic!("expected example.com"),
        }
        let tls = u_serde.tls.as_ref().unwrap();
        assert_eq!(tls.sni.as_deref(), Some("example.com"));
        assert!(tls.verify_hostname.unwrap());
    }

    #[test]
    fn from_runtime_tls_variants() {
        use pavis_core::{
            ClientCert, ConnectTimeout, ConnectionLimit, Discovery, HttpVersion, IdleTimeout,
            LoadBalancer, Pool, SniName, TlsPolicy, TlsVerify, UpstreamId, UpstreamName,
        };

        let mut upstream = pavis_core::Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("u".to_string()),
            discovery: Discovery::Static,
            balancer: LoadBalancer::Random,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Enabled {
                mode: TlsVerify::Disabled,
                sni: SniName::Auto,
                cert: ClientCert::Disabled,
            },
            endpoints: vec![],
        };

        // 1. TlsVerify::Disabled, SniName::Auto
        let serde = from_runtime(vec![upstream.clone()]);
        let tls = serde[0].tls.as_ref().unwrap();
        assert!(!tls.verify_cert.unwrap());
        assert!(!tls.verify_hostname.unwrap());
        assert_eq!(tls.sni, None);

        // 2. TlsVerify::Cert
        if let TlsPolicy::Enabled { mode, .. } = &mut upstream.tls {
            *mode = TlsVerify::Cert;
        }
        let serde = from_runtime(vec![upstream.clone()]);
        let tls = serde[0].tls.as_ref().unwrap();
        assert!(tls.verify_cert.unwrap());
        assert!(!tls.verify_hostname.unwrap());

        // 3. Pool::Limited
        upstream.pool.max = ConnectionLimit::Limited(NonZeroU32::new(100).unwrap());
        let serde = from_runtime(vec![upstream.clone()]);
        assert_eq!(serde[0].pool.as_ref().unwrap().max, Some(100));
    }
}
