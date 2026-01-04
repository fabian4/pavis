use anyhow::Result;
use pavis_core::{DiscoveryType, Endpoint, EndpointAddress, Upstream};
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
                config.discovery_type,
                DiscoveryType::LogicalDns | DiscoveryType::StrictDns
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
    let result = match config.discovery_type {
        DiscoveryType::LogicalDns => resolve_logical_dns(&config, &current).await,
        DiscoveryType::StrictDns => resolve_strict_dns(&config).await,
        DiscoveryType::Static => return None,
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
        .filter(|e| matches!(e.address, EndpointAddress::Dns(_, _)))
        .collect();

    if dns_endpoints.is_empty() {
        tracing::warn!(
            upstream = %config.name,
            "Logical DNS upstream has no DNS endpoints"
        );
        return Ok(None);
    }

    if dns_endpoints.len() > 1 || config.endpoints.len() != dns_endpoints.len() {
        tracing::warn!(
            upstream = %config.name,
            "Logical DNS upstream should define exactly one DNS endpoint"
        );
    }

    let endpoint = dns_endpoints[0];
    let (host, port) = match &endpoint.address {
        EndpointAddress::Dns(host, port) => (host.as_str(), *port),
        _ => unreachable!("filtered to DNS endpoints"),
    };

    let resolved = resolve_dns(host, port).await?;
    if resolved.is_empty() {
        return Ok(None);
    }

    let selected = select_existing_or_first(&resolved, current).unwrap_or_else(|| resolved[0]);
    let new_endpoint = Endpoint {
        address: EndpointAddress::Ip(selected),
        weight: endpoint.weight,
    };

    Ok(Some(vec![new_endpoint]))
}

async fn resolve_strict_dns(config: &Upstream) -> Result<Option<Vec<Endpoint>>> {
    let mut resolved_endpoints = Vec::new();

    for endpoint in &config.endpoints {
        match &endpoint.address {
            EndpointAddress::Ip(addr) => resolved_endpoints.push(Endpoint {
                address: EndpointAddress::Ip(*addr),
                weight: endpoint.weight,
            }),
            EndpointAddress::Dns(host, port) => {
                let addrs = resolve_dns(host, *port).await?;
                if addrs.is_empty() {
                    return Ok(None);
                }
                for addr in addrs {
                    resolved_endpoints.push(Endpoint {
                        address: EndpointAddress::Ip(addr),
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
        if let EndpointAddress::Ip(addr) = endpoint.address
            && resolved.contains(&addr)
        {
            return Some(addr);
        }
    }
    None
}
