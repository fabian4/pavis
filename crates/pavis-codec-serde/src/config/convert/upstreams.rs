use anyhow::{Context, Result};
use std::net::IpAddr;
use std::num::{NonZeroU16, NonZeroU32};

use pavis_core::{ConnectTimeout, Discovery, EndpointAddr, TlsVerify};

use crate::config::types::{Endpoint, Upstream, UpstreamTlsConfig};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{ConnectionPoolConfig, Endpoint, Upstream, UpstreamTlsConfig};
    use pavis_core::{Discovery, HttpVersion, LoadBalancer, TlsPolicy};
    use std::time::Duration;

    fn base_upstream() -> Upstream {
        Upstream {
            id: None,
            name: "u1".to_string(),
            discovery: Some(Discovery::default()),
            balancer: Some(LoadBalancer::default()),
            protocol: Some(HttpVersion::default()),
            pool: Some(ConnectionPoolConfig::default()),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints: Vec::new(),
        }
    }

    #[test]
    fn to_runtime_validates_port() {
        let upstream = Upstream {
            name: "u1".to_string(),
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 0,
                weight: None,
            }],
            ..base_upstream()
        };
        let err = to_runtime(vec![upstream]).unwrap_err();
        assert!(err.to_string().contains("endpoint port must be > 0"));
    }

    #[test]
    fn to_runtime_validates_ip_address() {
        let upstream = Upstream {
            name: "u1".to_string(),
            discovery: Some(Discovery::Static),
            endpoints: vec![Endpoint {
                address: "invalid-ip".to_string(),
                port: 8080,
                weight: None,
            }],
            ..base_upstream()
        };
        let err = to_runtime(vec![upstream]).unwrap_err();
        assert!(err.to_string().contains("Invalid endpoint IP"));
    }

    #[test]
    fn to_runtime_validates_weight() {
        let upstream = Upstream {
            name: "u1".to_string(),
            endpoints: vec![Endpoint {
                address: "127.0.0.1".to_string(),
                port: 8080,
                weight: Some(0),
            }],
            ..base_upstream()
        };
        let err = to_runtime(vec![upstream]).unwrap_err();
        assert!(err.to_string().contains("endpoint weight must be > 0"));
    }

    #[test]
    fn to_runtime_validates_pool_limits() {
        let upstream = Upstream {
            name: "u1".to_string(),
            pool: Some(ConnectionPoolConfig {
                idle: Some(Duration::from_millis(u64::MAX)),
                connect: Some(Duration::from_secs(1)),
                max: None,
            }),
            ..base_upstream()
        };
        let err = to_runtime(vec![upstream]).unwrap_err();
        assert!(err.to_string().contains("idle timeout exceeds u32::MAX"));
    }

    #[test]
    fn to_runtime_handles_tls_config() {
        let upstream = Upstream {
            name: "u1".to_string(),
            tls: Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(false),
                verify_cert: Some(true),
                sni: Some("example.com".to_string()),
            }),
            ..base_upstream()
        };
        let runtime = to_runtime(vec![upstream]).unwrap();
        match &runtime[0].tls {
            TlsPolicy::Enabled { verify_mode, sni } => {
                assert!(matches!(verify_mode, pavis_core::TlsVerify::Cert));
                assert!(matches!(sni, pavis_core::SniName::Value(h) if h.0 == "example.com"));
            }
            _ => panic!("expected enabled tls"),
        }
    }

    #[test]
    fn from_runtime_round_trips_full_config() {
        use pavis_core::*;
        let runtime = pavis_core::Upstream {
            id: UpstreamId(std::num::NonZeroU16::new(10).unwrap()),
            name: UpstreamName("u1".to_string()),
            discovery: Discovery::StrictDns,
            balancer: LoadBalancer::LeastRequest,
            protocol: HttpVersion::H2,
            pool: Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Disabled,
            endpoints: vec![pavis_core::Endpoint {
                address: EndpointAddr::Dns {
                    host: Hostname("example.com".to_string()),
                    port: Port(std::num::NonZeroU16::new(443).unwrap()),
                },
                weight: Weight(std::num::NonZeroU16::new(5).unwrap()),
            }],
        };

        let serde = from_runtime(vec![runtime]);
        assert_eq!(serde[0].endpoints[0].address, "example.com");
        assert_eq!(serde[0].endpoints[0].port, 443);
        assert_eq!(serde[0].endpoints[0].weight, Some(5));
        assert_eq!(serde[0].protocol, Some(HttpVersion::H2));
    }

    #[test]
    fn to_runtime_handles_discovery_variants() {
        let upstreams = vec![
            Upstream {
                name: "dns-logical".to_string(),
                discovery: Some(Discovery::LogicalDns),
                endpoints: vec![Endpoint {
                    address: "example.com".to_string(),
                    port: 80,
                    weight: None,
                }],
                ..base_upstream()
            },
            Upstream {
                name: "dns-strict".to_string(),
                discovery: Some(Discovery::StrictDns),
                endpoints: vec![Endpoint {
                    address: "example.com".to_string(),
                    port: 80,
                    weight: None,
                }],
                ..base_upstream()
            },
        ];
        let runtime = to_runtime(upstreams).unwrap();
        assert!(matches!(runtime[0].discovery, Discovery::LogicalDns));
        assert!(matches!(runtime[1].discovery, Discovery::StrictDns));
    }

    #[test]
    fn to_runtime_handles_pool_max_and_id() {
        let upstream = Upstream {
            id: Some(42),
            name: "u1".to_string(),
            pool: Some(ConnectionPoolConfig {
                idle: None,
                connect: None,
                max: Some(100),
            }),
            ..base_upstream()
        };
        let runtime = to_runtime(vec![upstream]).unwrap();
        assert_eq!(runtime[0].id.0.get(), 42);
        match runtime[0].pool.max {
            pavis_core::ConnectionLimit::Limited(n) => assert_eq!(n.get(), 100),
            _ => panic!("expected limited pool"),
        }
    }

    #[test]
    fn to_runtime_handles_tls_disabled_explicitly() {
        let upstream = Upstream {
            name: "u1".to_string(),
            tls: Some(UpstreamTlsConfig {
                enabled: Some(false),
                verify_hostname: None,
                verify_cert: None,
                sni: None,
            }),
            ..base_upstream()
        };
        let runtime = to_runtime(vec![upstream]).unwrap();
        assert!(matches!(runtime[0].tls, pavis_core::TlsPolicy::Disabled));
    }
}

pub(super) fn to_runtime(upstreams: Vec<Upstream>) -> Result<Vec<pavis_core::Upstream>> {
    let mut runtime_upstreams = Vec::new();

    for (index, u) in upstreams.into_iter().enumerate() {
        let mut endpoints = Vec::new();
        let discovery = u.discovery.unwrap_or_default();
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
                Discovery::LogicalDns | Discovery::StrictDns => EndpointAddr::Dns {
                    host: pavis_core::Hostname(e.address),
                    port: pavis_core::Port(port),
                },
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

        let pool_config = u.pool.unwrap_or_default();
        let idle = duration_to_policy(
            pool_config
                .idle
                .unwrap_or(std::time::Duration::from_secs(60)),
        )?;
        let connect = duration_to_connect(
            pool_config
                .connect
                .unwrap_or(std::time::Duration::from_secs(5)),
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
                if !t.enabled.unwrap_or(false) {
                    pavis_core::TlsPolicy::Disabled
                } else {
                    let verify_cert = t.verify_cert.unwrap_or(true);
                    let verify_hostname = t.verify_hostname.unwrap_or(true);
                    let verify_mode = match (verify_cert, verify_hostname) {
                        (false, _) => TlsVerify::Disabled,
                        (true, false) => TlsVerify::Cert,
                        (true, true) => TlsVerify::CertAndHost,
                    };
                    let sni = match t.sni {
                        Some(name) => pavis_core::SniName::Value(pavis_core::Hostname(name)),
                        None => pavis_core::SniName::Auto,
                    };
                    pavis_core::TlsPolicy::Enabled { verify_mode, sni }
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
            balancer: u.balancer.unwrap_or_default(),
            protocol: u.protocol.unwrap_or_default(),
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
            };
            endpoints.push(Endpoint {
                address,
                port,
                weight: Some(e.weight.0.get() as u32),
            });
        }

        let pool = crate::config::types::ConnectionPoolConfig {
            idle: Some(std::time::Duration::from_millis(idle_timeout_ms(
                &u.pool.idle,
            ))),
            connect: Some(std::time::Duration::from_millis(connect_timeout_ms(
                &u.pool.connect,
            ))),
            max: match u.pool.max {
                pavis_core::ConnectionLimit::Unlimited => None,
                pavis_core::ConnectionLimit::Limited(value) => Some(value.get()),
            },
        };

        let tls = match u.tls {
            pavis_core::TlsPolicy::Disabled => None,
            pavis_core::TlsPolicy::Enabled { verify_mode, sni } => {
                let (verify_cert, verify_hostname) = match verify_mode {
                    TlsVerify::Disabled => (false, false),
                    TlsVerify::Cert => (true, false),
                    TlsVerify::CertAndHost => (true, true),
                };
                let sni = match sni {
                    pavis_core::SniName::Auto => None,
                    pavis_core::SniName::Value(name) => Some(name.0),
                };
                Some(UpstreamTlsConfig {
                    enabled: Some(true),
                    verify_hostname: Some(verify_hostname),
                    verify_cert: Some(verify_cert),
                    sni,
                })
            }
        };

        serde_upstreams.push(Upstream {
            id: Some(u.id.0.get()),
            name: u.name.0,
            discovery: Some(u.discovery),
            balancer: Some(u.balancer),
            protocol: Some(u.protocol),
            pool: Some(pool),
            tls,
            circuit_breaker: None,
            health_check: None,
            endpoints,
        });
    }

    serde_upstreams
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
    }
}

fn connect_timeout_ms(timeout: &pavis_core::ConnectTimeout) -> u64 {
    match timeout {
        pavis_core::ConnectTimeout::Disabled => 0,
        pavis_core::ConnectTimeout::Enabled(d) => d.0.get() as u64,
    }
}
