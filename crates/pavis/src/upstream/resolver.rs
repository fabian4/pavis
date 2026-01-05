use anyhow::Result;
use pavis_core::{Discovery, Endpoint, EndpointAddr, Upstream};
use pingora::services::Service;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinSet;

use crate::state::RuntimeStateHandle;

pub struct UpstreamResolver {
    state: Arc<RuntimeStateHandle>,
    interval: Duration,
}

struct ResolvedUpdate {
    name: String,
    endpoints: Vec<Endpoint>,
}

impl UpstreamResolver {
    pub fn new(state: Arc<RuntimeStateHandle>, interval: Duration) -> Self {
        Self { state, interval }
    }

    async fn resolve_once(&self) {
        let state = self.state.load();
        let mut join_set = JoinSet::new();

        for (name, cluster) in state.upstream_manager.iter() {
            let config = cluster.config.clone();
            let current = cluster.current_endpoints();
            if !matches!(
                config.discovery,
                Discovery::LogicalDns | Discovery::StrictDns
            ) {
                continue;
            }

            let name = name.clone();
            join_set.spawn(async move { resolve_upstream(name, config, current).await });
        }

        drop(state);

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Some(update)) => {
                    let state = self.state.load();
                    if let Some(cluster) = state.upstream_manager.get(&update.name) {
                        cluster.update_endpoints(update.endpoints);
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(error = %err, "DNS resolution task failed");
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl Service for UpstreamResolver {
    async fn start_service(
        &mut self,
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: watch::Receiver<bool>,
        _threads: usize,
    ) {
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = ticker.tick() => {
                    self.resolve_once().await;
                }
            }
        }
    }

    fn name(&self) -> &str {
        "upstream_resolver"
    }
}

async fn resolve_upstream(
    name: String,
    config: Upstream,
    current: Vec<Endpoint>,
) -> Option<ResolvedUpdate> {
    let result = match config.discovery {
        Discovery::LogicalDns => resolve_logical_dns(&config, &current).await,
        Discovery::StrictDns => resolve_strict_dns(&config).await,
        Discovery::Static => return None,
    };

    match result {
        Ok(Some(endpoints)) => {
            tracing::info!(
                upstream = %name,
                endpoint_count = endpoints.len(),
                "DNS resolution updated upstream"
            );
            Some(ResolvedUpdate { name, endpoints })
        }
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(upstream = %name, error = %err, "DNS resolution failed");
            None
        }
    }
}

async fn resolve_logical_dns(
    config: &Upstream,
    current: &[Endpoint],
) -> Result<Option<Vec<Endpoint>>> {
    let dns_endpoints: Vec<&Endpoint> = config
        .endpoints
        .iter()
        .filter(|e| matches!(e.address, EndpointAddr::Dns { .. }))
        .collect();

    if dns_endpoints.is_empty() {
        tracing::warn!(
            upstream = %config.name.0,
            "Logical DNS upstream has no DNS endpoints"
        );
        return Ok(None);
    }

    if dns_endpoints.len() > 1 || config.endpoints.len() != dns_endpoints.len() {
        tracing::warn!(
            upstream = %config.name.0,
            "Logical DNS upstream should define exactly one DNS endpoint"
        );
    }

    let endpoint = dns_endpoints[0];
    let (host, port) = match &endpoint.address {
        EndpointAddr::Dns { host, port } => (host.0.as_str(), port.0.get()),
        _ => unreachable!("filtered to DNS endpoints"),
    };

    let resolved = resolve_dns(host, port).await?;
    if resolved.is_empty() {
        return Ok(None);
    }

    let selected = select_existing_or_first(&resolved, current).unwrap_or_else(|| resolved[0]);
    let new_endpoint = Endpoint {
        address: EndpointAddr::Ip {
            address: selected.ip(),
            port: endpoint_port(selected),
        },
        weight: endpoint.weight,
    };

    Ok(Some(vec![new_endpoint]))
}

async fn resolve_strict_dns(config: &Upstream) -> Result<Option<Vec<Endpoint>>> {
    let mut resolved_endpoints = Vec::new();

    for endpoint in &config.endpoints {
        match &endpoint.address {
            EndpointAddr::Ip { address, port } => resolved_endpoints.push(Endpoint {
                address: EndpointAddr::Ip {
                    address: *address,
                    port: *port,
                },
                weight: endpoint.weight,
            }),
            EndpointAddr::Dns { host, port } => {
                let addrs = resolve_dns(host.0.as_str(), port.0.get()).await?;
                if addrs.is_empty() {
                    return Ok(None);
                }
                for addr in addrs {
                    resolved_endpoints.push(Endpoint {
                        address: EndpointAddr::Ip {
                            address: addr.ip(),
                            port: endpoint_port(addr),
                        },
                        weight: endpoint.weight,
                    });
                }
            }
        }
    }

    if resolved_endpoints.is_empty() {
        return Ok(None);
    }

    Ok(Some(resolved_endpoints))
}

async fn resolve_dns(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let result = tokio::net::lookup_host((host, port)).await?;
    Ok(result.collect())
}

fn select_existing_or_first(resolved: &[SocketAddr], current: &[Endpoint]) -> Option<SocketAddr> {
    for endpoint in current {
        if let EndpointAddr::Ip { address, port } = endpoint.address {
            let addr = SocketAddr::new(address, port.0.get());
            if resolved.contains(&addr) {
                return Some(addr);
            }
        }
    }
    None
}

fn endpoint_port(addr: SocketAddr) -> pavis_core::Port {
    use std::num::NonZeroU16;
    pavis_core::Port(NonZeroU16::new(addr.port()).expect("non-zero port"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{Port, Weight};
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU16;

    #[test]
    fn test_endpoint_port() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let port = endpoint_port(addr);
        assert_eq!(port.0.get(), 8080);
    }

    #[test]
    fn test_select_existing_or_first() {
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8080);
        let resolved = vec![addr1, addr2];

        // Case 1: Current is empty
        assert_eq!(select_existing_or_first(&resolved, &[]), None);

        // Case 2: Current matches one of resolved
        let current = vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                port: Port(NonZeroU16::new(8080).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }];
        assert_eq!(select_existing_or_first(&resolved, &current), Some(addr2));

        // Case 3: Current matches nothing in resolved
        let current_mismatch = vec![Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3)),
                port: Port(NonZeroU16::new(8080).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        }];
        assert_eq!(select_existing_or_first(&resolved, &current_mismatch), None);
    }

    #[tokio::test]
    async fn test_resolve_logical_dns_no_dns_endpoints() {
        use pavis_core::{Discovery, HttpVersion, LoadBalancer, Pool, UpstreamId, UpstreamName};
        let config = Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("test".to_string()),
            discovery: Discovery::LogicalDns,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: pavis_core::IdleTimeout::Disabled,
                connect: pavis_core::ConnectTimeout::Disabled,
                max: pavis_core::ConnectionLimit::Unlimited,
            },
            tls: pavis_core::TlsPolicy::Disabled,
            endpoints: vec![Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        };

        let result = resolve_logical_dns(&config, &[]).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_resolve_strict_dns_empty_endpoints() {
        use pavis_core::{Discovery, HttpVersion, LoadBalancer, Pool, UpstreamId, UpstreamName};
        let config = Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("test".to_string()),
            discovery: Discovery::StrictDns,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: pavis_core::IdleTimeout::Disabled,
                connect: pavis_core::ConnectTimeout::Disabled,
                max: pavis_core::ConnectionLimit::Unlimited,
            },
            tls: pavis_core::TlsPolicy::Disabled,
            endpoints: vec![],
        };

        let result = resolve_strict_dns(&config).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_resolve_upstream_static() {
        use pavis_core::{Discovery, HttpVersion, LoadBalancer, Pool, UpstreamId, UpstreamName};
        let config = Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("test".to_string()),
            discovery: Discovery::Static,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: pavis_core::IdleTimeout::Disabled,
                connect: pavis_core::ConnectTimeout::Disabled,
                max: pavis_core::ConnectionLimit::Unlimited,
            },
            tls: pavis_core::TlsPolicy::Disabled,
            endpoints: vec![],
        };

        let result = resolve_upstream("test".to_string(), config, vec![]).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_resolve_logical_dns_multiple_endpoints_warning() {
        use pavis_core::{Discovery, HttpVersion, LoadBalancer, Pool, UpstreamId, UpstreamName};
        let config = Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("test".to_string()),
            discovery: Discovery::LogicalDns,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: pavis_core::IdleTimeout::Disabled,
                connect: pavis_core::ConnectTimeout::Disabled,
                max: pavis_core::ConnectionLimit::Unlimited,
            },
            tls: pavis_core::TlsPolicy::Disabled,
            endpoints: vec![
                Endpoint {
                    address: EndpointAddr::Dns {
                        host: pavis_core::Hostname("localhost".to_string()),
                        port: pavis_core::Port(NonZeroU16::new(8080).unwrap()),
                    },
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                },
                Endpoint {
                    address: EndpointAddr::Dns {
                        host: pavis_core::Hostname("localhost".to_string()),
                        port: pavis_core::Port(NonZeroU16::new(8081).unwrap()),
                    },
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                },
            ],
        };

        // This should still work but hit the warning branch
        let result = resolve_logical_dns(&config, &[]).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_resolve_dns_success() {
        let res = resolve_dns("localhost", 80).await.unwrap();
        assert!(!res.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_dns_failure() {
        // Empty host should fail
        let res = resolve_dns("", 80).await;
        assert!(res.is_err());
    }
}
