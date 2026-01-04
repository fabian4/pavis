use anyhow::{Context, Result};
use std::net::IpAddr;
use std::num::{NonZeroU16, NonZeroU32};

use pavis_core::{ConnectTimeout, Discovery, EndpointAddr, TlsVerify};

use crate::config::types::{Endpoint, Upstream, UpstreamTlsConfig};

pub(super) fn to_runtime(upstreams: Vec<Upstream>) -> Result<Vec<pavis_core::Upstream>> {
    let mut runtime_upstreams = Vec::new();

    for (index, u) in upstreams.into_iter().enumerate() {
        let mut endpoints = Vec::new();
        for e in u.endpoints {
            let port = NonZeroU16::new(e.port)
                .ok_or_else(|| anyhow::anyhow!("endpoint port must be > 0"))?;
            let address = match u.discovery {
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

        let idle = duration_to_policy(u.pool.idle)?;
        let connect = duration_to_connect(u.pool.connect)?;
        let max = match u.pool.max {
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
                if !t.enabled {
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
            discovery: u.discovery,
            balancer: u.balancer,
            protocol: u.protocol,
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
            idle: std::time::Duration::from_millis(idle_timeout_ms(&u.pool.idle)),
            connect: std::time::Duration::from_millis(connect_timeout_ms(&u.pool.connect)),
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
                    enabled: true,
                    verify_hostname: Some(verify_hostname),
                    verify_cert: Some(verify_cert),
                    sni,
                })
            }
        };

        serde_upstreams.push(Upstream {
            id: Some(u.id.0.get()),
            name: u.name.0,
            discovery: u.discovery,
            balancer: u.balancer,
            protocol: u.protocol,
            pool,
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
