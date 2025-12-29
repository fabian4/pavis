use anyhow::{Context, Result};
use std::net::IpAddr;

use crate::config::types::{Endpoint, Upstream, UpstreamTlsConfig};

pub(super) fn to_runtime(upstreams: Vec<Upstream>) -> Result<Vec<pavis_core::Upstream>> {
    let mut runtime_upstreams = Vec::new();

    for u in upstreams {
        let mut endpoints = Vec::new();
        for e in u.endpoints {
            let ip: IpAddr = e.ip.parse().with_context(|| {
                format!("Invalid endpoint IP '{}' for upstream '{}'", e.ip, u.name)
            })?;
            endpoints.push(pavis_core::Endpoint {
                ip,
                port: e.port,
                weight: e.weight.unwrap_or(1),
            });
        }

        let connection_pool = pavis_core::ConnectionPoolConfig {
            idle_timeout_secs: u.connection_pool.idle_timeout.as_secs(),
            connection_timeout_secs: u.connection_pool.connection_timeout.as_secs(),
        };

        let tls = u.tls.map(|t| pavis_core::UpstreamTlsConfig {
            enabled: t.enabled,
            verify_hostname: t.verify_hostname.unwrap_or(true),
            verify_cert: t.verify_cert.unwrap_or(true),
            sni: t.sni,
        });

        runtime_upstreams.push(pavis_core::Upstream {
            name: u.name,
            load_balancer: u.load_balancer,
            http_version: u.http_version,
            connection_pool,
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
            endpoints.push(Endpoint {
                ip: e.ip.to_string(),
                port: e.port,
                weight: Some(e.weight),
            });
        }

        let connection_pool = crate::config::types::ConnectionPoolConfig {
            idle_timeout: std::time::Duration::from_secs(u.connection_pool.idle_timeout_secs),
            connection_timeout: std::time::Duration::from_secs(
                u.connection_pool.connection_timeout_secs,
            ),
        };

        let tls = u.tls.map(|t| UpstreamTlsConfig {
            enabled: t.enabled,
            verify_hostname: Some(t.verify_hostname),
            verify_cert: Some(t.verify_cert),
            sni: t.sni,
        });

        serde_upstreams.push(Upstream {
            name: u.name,
            load_balancer: u.load_balancer,
            http_version: u.http_version,
            connection_pool,
            tls,
            circuit_breaker: None,
            health_check: None,
            endpoints,
        });
    }

    serde_upstreams
}
