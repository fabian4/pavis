use crate::regex_validator::validate_and_compile_regexes;
use crate::router::Router;
use crate::upstream::Manager;
use anyhow::Context;
use arc_swap::ArcSwap;
use pavis_core::{
    AccessLogPolicy, ConfigVersion, Discovery, Endpoint, EndpointAddr, Hostname, ListenerBuilder,
    ListenerName, Metrics, Port, RuntimeConfigBuilder, ServiceName, Telemetry, Upstream,
    ValidatedRuntimeConfig, WorkerCount,
};
use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::ops::Deref;
use std::sync::Arc;
use tracing::warn;

pub struct MaterializedRuntimeConfig {
    pub router: Arc<Router>,
    pub upstream_manager: Manager,
}

impl MaterializedRuntimeConfig {
    pub fn build(config: &ValidatedRuntimeConfig) -> anyhow::Result<Self> {
        let regex_limits = config.features.regex_limits.clone();
        let regex_cache = validate_and_compile_regexes(config, &regex_limits)
            .map_err(|e| anyhow::anyhow!("Regex validation failed: {}", e))?;
        let router = Arc::new(Router::with_regex(
            config.routes.clone(),
            regex_cache,
            regex_limits,
        )?);
        let resolved_endpoints = materialize_upstreams(&config.upstreams)?;
        let upstream_manager = Manager::new(&config.upstreams)?;
        for (name, endpoints) in &resolved_endpoints {
            if let Some(cluster) = upstream_manager.get(name) {
                cluster.update_endpoints(endpoints.clone());
            }
        }
        Ok(Self {
            router,
            upstream_manager,
        })
    }

    pub fn from_components(router: Arc<Router>, upstream_manager: Manager) -> Self {
        Self {
            router,
            upstream_manager,
        }
    }

    fn empty() -> Self {
        Self {
            router: Arc::new(Router::new(vec![]).expect("empty router")),
            upstream_manager: Manager::new(&[]).expect("empty upstream manager"),
        }
    }
}

pub struct RuntimeState {
    pub config: ValidatedRuntimeConfig,
    materialized: Arc<MaterializedRuntimeConfig>,
    pub config_version: Option<ConfigVersion>,
}

impl RuntimeState {
    pub fn from_config(config: &ValidatedRuntimeConfig) -> anyhow::Result<Self> {
        let materialized = MaterializedRuntimeConfig::build(config)?;
        Ok(Self {
            config: config.clone(),
            materialized: Arc::new(materialized),
            config_version: None,
        })
    }

    pub fn with_components(
        config: ValidatedRuntimeConfig,
        router: Arc<Router>,
        upstream_manager: Manager,
    ) -> Self {
        Self {
            config,
            materialized: Arc::new(MaterializedRuntimeConfig::from_components(
                router,
                upstream_manager,
            )),
            config_version: None,
        }
    }

    pub fn materialized(&self) -> &MaterializedRuntimeConfig {
        &self.materialized
    }
}

impl Deref for RuntimeState {
    type Target = MaterializedRuntimeConfig;

    fn deref(&self) -> &Self::Target {
        &self.materialized
    }
}

impl Default for RuntimeState {
    fn default() -> Self {
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address("127.0.0.1:0".parse().expect("default addr"))
            .workers(WorkerCount::Auto)
            .tls(pavis_core::TlsConfig::Disabled)
            .build()
            .expect("listener");

        let empty_config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Error,
                service_name: ServiceName("pavis".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(listener)
            .build()
            .expect("config");
        // SAFETY: Default RuntimeConfig is empty and valid.
        let config = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(empty_config) };
        Self {
            config,
            materialized: Arc::new(MaterializedRuntimeConfig::empty()),
            config_version: None,
        }
    }
}

pub struct RuntimeStateHandle {
    inner: ArcSwap<RuntimeState>,
}

impl RuntimeStateHandle {
    pub fn new(state: RuntimeState) -> Self {
        Self {
            inner: ArcSwap::from_pointee(state),
        }
    }

    pub fn load(&self) -> Arc<RuntimeState> {
        self.inner.load_full()
    }

    pub fn store(&self, state: RuntimeState) {
        self.inner.store(Arc::new(state));
    }
}

fn materialize_upstreams(upstreams: &[Upstream]) -> anyhow::Result<HashMap<String, Vec<Endpoint>>> {
    let mut resolved = HashMap::new();
    for upstream in upstreams {
        let endpoints = match upstream.discovery {
            Discovery::Logical => materialize_logical_upstream(upstream)?,
            _ => materialize_all_endpoints(upstream)?,
        };
        resolved.insert(upstream.name.0.clone(), endpoints);
    }
    Ok(resolved)
}

fn materialize_logical_upstream(upstream: &Upstream) -> anyhow::Result<Vec<Endpoint>> {
    if let Some(ip_endpoint) = upstream
        .endpoints
        .iter()
        .find(|endpoint| matches!(endpoint.address, EndpointAddr::Ip { .. }))
    {
        return Ok(vec![ip_endpoint.clone()]);
    }

    let dns_endpoint = upstream
        .endpoints
        .iter()
        .find(|endpoint| matches!(endpoint.address, EndpointAddr::Dns { .. }))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Logical DNS upstream {} must define at least one DNS endpoint",
                upstream.name.0
            )
        })?;

    let mut resolved = materialize_endpoint(dns_endpoint)?;
    if resolved.is_empty() {
        anyhow::bail!(
            "DNS resolution returned no addresses for logical upstream {}",
            upstream.name.0
        );
    }
    if resolved.len() > 1 {
        warn!(
            upstream = %upstream.name.0,
            resolved = resolved.len(),
            "Logical DNS upstream resolved to multiple addresses; selecting the first"
        );
    }
    Ok(vec![resolved.remove(0)])
}

fn materialize_all_endpoints(upstream: &Upstream) -> anyhow::Result<Vec<Endpoint>> {
    let mut resolved = Vec::new();
    for endpoint in &upstream.endpoints {
        resolved.extend(materialize_endpoint(endpoint)?);
    }
    if resolved.is_empty() {
        anyhow::bail!("Upstream {} has no resolvable endpoints", upstream.name.0);
    }
    Ok(resolved)
}

fn materialize_endpoint(endpoint: &Endpoint) -> anyhow::Result<Vec<Endpoint>> {
    match &endpoint.address {
        EndpointAddr::Ip { .. } => Ok(vec![endpoint.clone()]),
        EndpointAddr::Dns { host, port } => {
            let addresses = resolve_dns_once(host, *port)?;
            Ok(addresses
                .into_iter()
                .map(|addr| Endpoint {
                    address: EndpointAddr::Ip {
                        address: addr.ip(),
                        port: *port,
                    },
                    weight: endpoint.weight,
                })
                .collect())
        }
        #[allow(unreachable_patterns)]
        _ => Ok(vec![endpoint.clone()]),
    }
}

fn resolve_dns_once(host: &Hostname, port: Port) -> anyhow::Result<Vec<SocketAddr>> {
    let addrs: Vec<SocketAddr> = (host.0.as_str(), port.0.get())
        .to_socket_addrs()
        .with_context(|| format!("DNS resolution failed for {}:{}", host.0, port.0.get()))?
        .collect();
    if addrs.is_empty() {
        anyhow::bail!(
            "DNS resolution returned no addresses for {}:{}",
            host.0,
            port.0.get()
        );
    }
    Ok(addrs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        Discovery, Endpoint, EndpointAddr, HttpVersion, LoadBalancer, Port, UpstreamBuilder,
        UpstreamId, UpstreamName, Weight,
    };
    use std::num::NonZeroU16;

    #[test]
    fn test_materialize_upstreams_empty() {
        let res = materialize_upstreams(&[]);
        assert!(res.is_ok());
        assert!(res.unwrap().is_empty());
    }

    #[test]
    fn test_materialize_static_ip() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: "127.0.0.1".parse().unwrap(),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        let res = materialize_upstreams(&[upstream]).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res["test"].len(), 1);
    }

    #[test]
    fn test_materialize_logical_dns_localhost() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Logical)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .add_endpoint(Endpoint {
                address: EndpointAddr::Dns {
                    host: Hostname("localhost".to_string()),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        let res = materialize_upstreams(&[upstream]).unwrap();
        assert_eq!(res.len(), 1);
        // localhost should resolve to at least 127.0.0.1 or ::1
        assert!(!res["test"].is_empty());
    }

    #[test]
    fn test_materialize_logical_dns_failure() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Logical)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .add_endpoint(Endpoint {
                address: EndpointAddr::Dns {
                    host: Hostname("nonexistent.invalid".to_string()),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        let res = materialize_upstreams(&[upstream]);
        assert!(res.is_err());
    }

    #[test]
    fn test_materialize_logical_dns_no_dns_endpoints() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Logical)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .build()
            .unwrap();

        let res = materialize_logical_upstream(&upstream);
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("must define at least one DNS endpoint")
        );
    }

    #[test]
    fn test_materialize_all_endpoints_failure() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .build()
            .unwrap();

        let res = materialize_all_endpoints(&upstream);
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("has no resolvable endpoints")
        );
    }

    #[test]
    fn test_runtime_state_with_components() {
        let router = Arc::new(Router::new(vec![]).unwrap());
        let manager = Manager::new(&[]).unwrap();
        let builder = RuntimeConfigBuilder::new();
        let config = builder
            .telemetry(Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Error,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(
                pavis_core::ListenerBuilder::new()
                    .name(pavis_core::ListenerName("test".to_string()))
                    .address("127.0.0.1:0".parse().unwrap())
                    .workers(pavis_core::WorkerCount::Auto)
                    .tls(pavis_core::TlsConfig::Disabled)
                    .build()
                    .unwrap(),
            )
            .build()
            .unwrap();
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(config) };

        let state = RuntimeState::with_components(validated, router, manager);
        assert!(
            state
                .router
                .match_request(
                    None,
                    "/",
                    "GET",
                    &pingora::http::RequestHeader::build("GET", b"/", None).unwrap()
                )
                .selection
                .is_none()
        );
    }

    #[test]
    fn test_runtime_state_handle() {
        let state = RuntimeState::default();
        let handle = RuntimeStateHandle::new(state);
        let loaded = handle.load();
        assert_eq!(loaded.config.telemetry.service_name.0, "pavis");

        let new_state = RuntimeState::default();
        handle.store(new_state);
    }
}
