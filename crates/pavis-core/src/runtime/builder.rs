use crate::runtime::{
    ConnectionLimit, Discovery, Endpoint, HttpVersion, IdleTimeout, Listener, ListenerName,
    LoadBalancer, Pool, RuntimeConfig, Telemetry, TlsConfig, TlsPolicy, Upstream, UpstreamId,
    UpstreamName, WorkerCount,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BuilderError {
    #[error("runtime config missing telemetry")]
    MissingTelemetry,
    #[error("runtime config has no listeners")]
    MissingListeners,
    #[error("listener missing address")]
    MissingListenerAddress,
    #[error("listener missing name")]
    MissingListenerName,
    #[error("upstream missing name")]
    MissingUpstreamName,
    #[error("upstream missing id")]
    MissingUpstreamId,
}

#[derive(Debug, Default)]
pub struct RuntimeConfigBuilder {
    listeners: Vec<Listener>,
    telemetry: Option<Telemetry>,
    upstreams: Vec<Upstream>,
    routes: Vec<crate::runtime::VirtualHost>,
}

impl RuntimeConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn telemetry(mut self, telemetry: Telemetry) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    pub fn add_listener(mut self, listener: Listener) -> Self {
        self.listeners.push(listener);
        self
    }

    pub fn add_upstream(mut self, upstream: Upstream) -> Self {
        self.upstreams.push(upstream);
        self
    }

    pub fn add_route(mut self, route: crate::runtime::VirtualHost) -> Self {
        self.routes.push(route);
        self
    }

    pub fn build(self) -> Result<RuntimeConfig, BuilderError> {
        if self.listeners.is_empty() {
            return Err(BuilderError::MissingListeners);
        }
        let telemetry = self.telemetry.ok_or(BuilderError::MissingTelemetry)?;
        Ok(RuntimeConfig {
            listeners: self.listeners,
            telemetry,
            upstreams: self.upstreams,
            routes: self.routes,
        })
    }
}

#[derive(Debug)]
pub struct ListenerBuilder {
    name: Option<ListenerName>,
    address: Option<std::net::SocketAddr>,
    workers: WorkerCount,
    tls: TlsConfig,
}

impl ListenerBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            address: None,
            workers: WorkerCount::Auto,
            tls: TlsConfig::Disabled,
        }
    }

    pub fn name(mut self, name: ListenerName) -> Self {
        self.name = Some(name);
        self
    }

    pub fn address(mut self, address: std::net::SocketAddr) -> Self {
        self.address = Some(address);
        self
    }

    pub fn workers(mut self, workers: WorkerCount) -> Self {
        self.workers = workers;
        self
    }

    pub fn tls(mut self, tls: TlsConfig) -> Self {
        self.tls = tls;
        self
    }

    pub fn build(self) -> Result<Listener, BuilderError> {
        let name = self.name.ok_or(BuilderError::MissingListenerName)?;
        let address = self.address.ok_or(BuilderError::MissingListenerAddress)?;
        Ok(Listener {
            name,
            address,
            workers: self.workers,
            tls: self.tls,
        })
    }
}

impl Default for ListenerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct UpstreamBuilder {
    id: Option<UpstreamId>,
    name: Option<UpstreamName>,
    discovery: Discovery,
    balancer: LoadBalancer,
    protocol: HttpVersion,
    pool: Pool,
    tls: TlsPolicy,
    endpoints: Vec<Endpoint>,
}

impl UpstreamBuilder {
    pub fn new() -> Self {
        Self {
            id: None,
            name: None,
            discovery: Discovery::Static,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: IdleTimeout::Disabled,
                connect: crate::runtime::ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Disabled,
            endpoints: Vec::new(),
        }
    }

    pub fn id(mut self, id: UpstreamId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn name(mut self, name: UpstreamName) -> Self {
        self.name = Some(name);
        self
    }

    pub fn discovery(mut self, discovery: Discovery) -> Self {
        self.discovery = discovery;
        self
    }

    pub fn balancer(mut self, balancer: LoadBalancer) -> Self {
        self.balancer = balancer;
        self
    }

    pub fn protocol(mut self, protocol: HttpVersion) -> Self {
        self.protocol = protocol;
        self
    }

    pub fn pool(mut self, pool: Pool) -> Self {
        self.pool = pool;
        self
    }

    pub fn tls(mut self, tls: TlsPolicy) -> Self {
        self.tls = tls;
        self
    }

    pub fn add_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    pub fn build(self) -> Result<Upstream, BuilderError> {
        let id = self.id.ok_or(BuilderError::MissingUpstreamId)?;
        let name = self.name.ok_or(BuilderError::MissingUpstreamName)?;
        Ok(Upstream {
            id,
            name,
            discovery: self.discovery,
            balancer: self.balancer,
            protocol: self.protocol,
            pool: self.pool,
            tls: self.tls,
            endpoints: self.endpoints,
        })
    }
}

impl Default for UpstreamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        AccessLogPolicy, EndpointAddr, LogLevel, Metrics, Port, SampleRate, ServiceName,
        TracingPolicy, TracingProvider, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU16;

    #[test]
    fn runtime_builder_requires_telemetry_listeners_upstreams() {
        let err = RuntimeConfigBuilder::new().build().unwrap_err();
        assert_eq!(err, BuilderError::MissingListeners);
    }

    #[test]
    fn listener_builder_requires_name_and_address() {
        let err = ListenerBuilder::new().build().unwrap_err();
        assert_eq!(err, BuilderError::MissingListenerName);
    }

    #[test]
    fn upstream_builder_requires_id_name_endpoints() {
        let err = UpstreamBuilder::new().build().unwrap_err();
        assert_eq!(err, BuilderError::MissingUpstreamId);
    }

    #[test]
    fn builder_happy_path_constructs_runtime_config() {
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address(std::net::SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                8080,
            ))
            .build()
            .expect("listener");

        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(unsafe { NonZeroU16::new_unchecked(1) }))
            .name(UpstreamName("backend".to_string()))
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(unsafe { NonZeroU16::new_unchecked(80) }),
                },
                weight: Weight(unsafe { NonZeroU16::new_unchecked(1) }),
            })
            .build()
            .expect("upstream");

        let telemetry = Telemetry {
            level: LogLevel::Info,
            pingora: LogLevel::Info,
            service_name: ServiceName("pavis".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: TracingPolicy::Enabled {
                provider: TracingProvider::Otlp,
                sampling: SampleRate(100),
                endpoint: "http://localhost:4317".to_string(),
            },
        };

        let cfg = RuntimeConfigBuilder::new()
            .telemetry(telemetry)
            .add_listener(listener)
            .add_upstream(upstream)
            .build()
            .expect("config");

        assert_eq!(cfg.listeners.len(), 1);
        assert_eq!(cfg.upstreams.len(), 1);
    }
}
