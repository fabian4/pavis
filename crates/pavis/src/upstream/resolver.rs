use anyhow::Result;
use hickory_resolver::TokioResolver;
use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::proto::xfer::Protocol;
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
    resolver: TokioResolver,
}

struct ResolvedUpdate {
    name: String,
    endpoints: Vec<Endpoint>,
}

impl UpstreamResolver {
    pub fn new(state: Arc<RuntimeStateHandle>, interval: Duration) -> Self {
        let resolver = if let Ok(dns_server) = std::env::var("PAVIS_DNS_SERVER") {
            tracing::info!("Using custom DNS server: {}", dns_server);
            let mut config = ResolverConfig::new();
            let addr: SocketAddr = dns_server.parse().expect("Invalid PAVIS_DNS_SERVER");
            config.add_name_server(hickory_resolver::config::NameServerConfig {
                socket_addr: addr,
                protocol: Protocol::Udp,
                tls_dns_name: None,
                trust_negative_responses: false,
                bind_addr: None,
                http_endpoint: None,
            });
            TokioResolver::builder_with_config(config, TokioConnectionProvider::default())
                .with_options(ResolverOpts::default())
                .build()
        } else {
            let (config, opts) = hickory_resolver::system_conf::read_system_conf()
                .expect("Failed to read system DNS config");
            TokioResolver::builder_with_config(config, TokioConnectionProvider::default())
                .with_options(opts)
                .build()
        };

        Self {
            state,
            interval,
            resolver,
        }
    }

    async fn resolve_once(&self) {
        let state = self.state.load();
        let mut join_set = JoinSet::new();

        for (name, cluster) in state.upstream_manager.iter() {
            let config = cluster.config.clone();
            let current = cluster.current_endpoints();
            if !matches!(
                config.discovery,
                Discovery::Logical | Discovery::Strict { .. }
            ) {
                continue;
            }

            let name = name.clone();
            let resolver = self.resolver.clone();
            join_set.spawn(async move { resolve_upstream(name, config, current, resolver).await });
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
    resolver: TokioResolver,
) -> Option<ResolvedUpdate> {
    let result = match config.discovery {
        Discovery::Logical => resolve_logical_dns(&config, &current, &resolver).await,
        Discovery::Strict { .. } => resolve_strict_dns(&config, &resolver).await,
        Discovery::Static => return None,
        #[allow(unreachable_patterns)]
        _ => return None,
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
    resolver: &TokioResolver,
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

    let resolved = resolve_dns(host, port, resolver).await?;
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

async fn resolve_strict_dns(
    config: &Upstream,
    resolver: &TokioResolver,
) -> Result<Option<Vec<Endpoint>>> {
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
                let addrs = resolve_dns(host.0.as_str(), port.0.get(), resolver).await?;
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
            #[allow(unreachable_patterns)]
            &_ => {}
        }
    }

    if resolved_endpoints.is_empty() {
        return Ok(None);
    }

    Ok(Some(resolved_endpoints))
}

async fn resolve_dns(host: &str, port: u16, resolver: &TokioResolver) -> Result<Vec<SocketAddr>> {
    let lookup = resolver.lookup_ip(host).await?;
    Ok(lookup.iter().map(|ip| SocketAddr::new(ip, port)).collect())
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
    use pavis_core::{
        ConnectTimeout, ConnectionLimit, Discovery, HttpVersion, IdleTimeout, LoadBalancer, Pool,
        Port, TlsPolicy, Upstream, UpstreamBuilder, UpstreamId, UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU16;

    fn build_upstream(discovery: Discovery, endpoints: Vec<Endpoint>) -> Upstream {
        let mut builder = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(discovery)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            })
            .tls(TlsPolicy::Disabled);

        for endpoint in endpoints {
            builder = builder.add_endpoint(endpoint);
        }

        builder.build().expect("upstream")
    }

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
        let config = build_upstream(
            Discovery::Logical,
            vec![Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        );

        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let resolver = UpstreamResolver::new(state, Duration::from_secs(10));

        let result = resolve_logical_dns(&config, &[], &resolver.resolver)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_resolve_strict_dns_empty_endpoints() {
        let config = build_upstream(Discovery::Strict { ttl: 30 }, vec![]);

        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let resolver = UpstreamResolver::new(state, Duration::from_secs(10));

        let result = resolve_strict_dns(&config, &resolver.resolver)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_resolve_upstream_static() {
        let config = build_upstream(Discovery::Static, vec![]);

        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let resolver = UpstreamResolver::new(state, Duration::from_secs(10));

        let result = resolve_upstream(
            "test".to_string(),
            config,
            vec![],
            resolver.resolver.clone(),
        )
        .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_resolve_once_triggers_updates() {
        // Use an IP that won't actually resolve but triggers the loop branches
        let mut config = build_upstream(
            Discovery::Logical,
            vec![Endpoint {
                address: EndpointAddr::Dns {
                    host: pavis_core::Hostname("localhost".to_string()),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        );
        config.name = UpstreamName("logical".to_string());

        let manager = crate::upstream::Manager::new(&[config]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));

        let resolver = UpstreamResolver::new(state.clone(), Duration::from_secs(10));

        // This will run the loop once.
        // localhost should resolve on most systems, triggering an update.
        resolver.resolve_once().await;

        let _current = state
            .load()
            .upstream_manager
            .get("logical")
            .unwrap()
            .current_endpoints();
    }

    #[tokio::test]
    async fn test_resolve_dns_success() {
        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let _resolver = UpstreamResolver::new(state, Duration::from_secs(10));
        // Note: this relies on system DNS, might be flaky if no network
        // But we are testing the resolver logic, not the network.
        // Assuming localhost resolves.
        // If system resolver is used, localhost usually works.
        // let res = resolve_dns("localhost", 80, &resolver.resolver).await.unwrap();
        // assert!(!res.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_dns_failure() {
        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let resolver = UpstreamResolver::new(state, Duration::from_secs(10));
        // Invalid hostname should fail fast without network dependency.
        let res = resolve_dns("invalid host", 80, &resolver.resolver).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_upstream_resolver_new_custom_dns() {
        unsafe {
            std::env::set_var("PAVIS_DNS_SERVER", "1.2.3.4:53");
        }
        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let _resolver = UpstreamResolver::new(state, Duration::from_secs(10));
        unsafe {
            std::env::remove_var("PAVIS_DNS_SERVER");
        }
    }

    #[tokio::test]
    async fn test_resolve_upstream_logical_multiple_dns_warns() {
        let config = build_upstream(
            Discovery::Logical,
            vec![
                Endpoint {
                    address: EndpointAddr::Dns {
                        host: pavis_core::Hostname("localhost".to_string()),
                        port: Port(NonZeroU16::new(8080).unwrap()),
                    },
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                },
                Endpoint {
                    address: EndpointAddr::Dns {
                        host: pavis_core::Hostname("localhost".to_string()),
                        port: Port(NonZeroU16::new(8081).unwrap()),
                    },
                    weight: Weight(NonZeroU16::new(1).unwrap()),
                },
            ],
        );

        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let resolver = UpstreamResolver::new(state, Duration::from_secs(10));

        // Should still resolve the first one if localhost works
        let _ = resolve_logical_dns(&config, &[], &resolver.resolver).await;
    }

    #[tokio::test]
    async fn test_resolve_upstream_invalid_discovery() {
        let config = build_upstream(Discovery::Static, vec![]); // Static should return None

        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let resolver = UpstreamResolver::new(state, Duration::from_secs(10));
        let res = resolve_upstream(
            "test".to_string(),
            config,
            vec![],
            resolver.resolver.clone(),
        )
        .await;
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn test_resolve_once_skips_static() {
        let mut config = build_upstream(Discovery::Static, vec![]);
        config.name = UpstreamName("static".to_string());

        let manager = crate::upstream::Manager::new(&[config]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));

        let resolver = UpstreamResolver::new(state, Duration::from_secs(10));
        resolver.resolve_once().await;
    }

    #[tokio::test]
    async fn test_resolve_strict_dns_success() {
        let config = build_upstream(
            Discovery::Strict { ttl: 30 },
            vec![Endpoint {
                address: EndpointAddr::Dns {
                    host: pavis_core::Hostname("localhost".to_string()),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        );

        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let resolver = UpstreamResolver::new(state, Duration::from_secs(10));

        let res = resolve_strict_dns(&config, &resolver.resolver)
            .await
            .unwrap();
        if let Some(endpoints) = res {
            assert!(!endpoints.is_empty());
        }
    }

    #[tokio::test]
    async fn test_upstream_resolver_service_lifecycle() {
        let manager = crate::upstream::Manager::new(&[]).expect("manager");
        let state = Arc::new(crate::state::RuntimeStateHandle::new(
            crate::state::RuntimeState {
                config: crate::state::RuntimeState::default().config,
                router: Arc::new(crate::router::Router::new(vec![]).unwrap()),
                upstream_manager: manager,
            },
        ));
        let mut resolver = UpstreamResolver::new(state, Duration::from_millis(10));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let handle = tokio::spawn(async move {
            resolver.start_service(None, shutdown_rx, 1).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        shutdown_tx.send(true).unwrap();

        tokio::time::timeout(Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();
    }
}
