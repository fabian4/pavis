mod builder;
mod headers;
mod routing;
mod server;
mod telemetry;
mod types;
mod upstream;

pub use builder::{BuilderError, ListenerBuilder, RuntimeConfigBuilder, UpstreamBuilder};
pub use headers::{Headers, HeadersPolicy};
pub use routing::{
    Destination, PathMatch, Principal, RETRY_CONNECT_FAILURE, RETRY_FIVE_XX, RETRY_REFUSED,
    RETRY_RESERVED, RETRY_RESET, RetryFlags, RetryPolicy, Rewrite, RewriteHost, RewritePath, Route,
    RouteAction, VirtualHost,
};
pub use server::{ClientAuth, Listener, TlsConfig, WorkerCount};
pub use telemetry::{
    AccessLogPolicy, LogLevel, Metrics, Telemetry, TracingPolicy, TracingProvider,
};
pub use types::{
    ConnectTimeout, Duration, HeaderName, HeaderValue, Host, Hostname, IdleTimeout, ListenerName,
    Path, Port, SampleRate, ServiceName, Timeout, TryTimeout, UpstreamId, UpstreamName, Weight,
};
pub use upstream::{
    ClientCert, ClientCertChain, ConnectionLimit, Discovery, Endpoint, EndpointAddr, HttpVersion,
    LoadBalancer, Pool, SniName, TlsPolicy, TlsVerify, Upstream, UpstreamCa,
};

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The Root Configuration Object.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub struct RuntimeConfig {
    pub listeners: Vec<Listener>,
    pub telemetry: Telemetry,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

#[derive(Debug, Clone)]
pub struct ValidatedRuntimeConfig {
    runtime: RuntimeConfig,
}

impl ValidatedRuntimeConfig {
    pub(crate) fn new(runtime: RuntimeConfig) -> Self {
        Self { runtime }
    }

    /// Construct a validated config without re-checking semantic invariants.
    ///
    /// # Safety
    /// Caller must guarantee the runtime config has already passed canonical validation.
    pub unsafe fn from_trusted(runtime: RuntimeConfig) -> Self {
        Self { runtime }
    }

    pub fn into_inner(self) -> RuntimeConfig {
        self.runtime
    }
}

impl AsRef<RuntimeConfig> for ValidatedRuntimeConfig {
    fn as_ref(&self) -> &RuntimeConfig {
        &self.runtime
    }
}

impl std::ops::Deref for ValidatedRuntimeConfig {
    type Target = RuntimeConfig;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::NonZeroU16;

    #[test]
    fn test_config_structure() {
        use std::sync::Arc;
        let config = RuntimeConfig {
            listeners: vec![Listener {
                name: ListenerName("default".to_string()),
                address: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080),
                workers: WorkerCount::Auto,
                tls: TlsConfig::Disabled,
            }],
            telemetry: Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Stdout,
                tracing: TracingPolicy::Disabled,
            },
            upstreams: vec![Upstream {
                id: UpstreamId(unsafe { NonZeroU16::new_unchecked(1) }),
                name: UpstreamName("upstream1".to_string()),
                discovery: Discovery::Static,
                balancer: LoadBalancer::RoundRobin,
                protocol: HttpVersion::H1,
                pool: Pool {
                    idle: IdleTimeout::Disabled,
                    connect: ConnectTimeout::Disabled,
                    max: ConnectionLimit::Unlimited,
                },
                tls: TlsPolicy::Disabled,
                endpoints: vec![Endpoint {
                    address: EndpointAddr::Ip {
                        address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                        port: Port(unsafe { NonZeroU16::new_unchecked(8080) }),
                    },
                    weight: Weight(unsafe { NonZeroU16::new_unchecked(1) }),
                }],
            }],
            routes: vec![VirtualHost {
                host: Host("*".to_string()),
                paths: vec![Route {
                    matcher: PathMatch::Prefix {
                        path: Path("/".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: Arc::new(HeadersPolicy::Disabled),
                    response_headers: Arc::new(HeadersPolicy::Disabled),
                    principal: Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("upstream1".to_string()),
                        weight: Weight(unsafe { NonZeroU16::new_unchecked(1) }),
                    }]),
                }],
            }],
        };

        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.routes.len(), 1);
    }

    #[test]
    fn validated_runtime_exposes_inner_config() {
        let config = RuntimeConfig {
            listeners: vec![Listener {
                name: ListenerName("default".to_string()),
                address: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080),
                workers: WorkerCount::Count(unsafe { NonZeroU16::new_unchecked(2) }),
                tls: TlsConfig::Disabled,
            }],
            telemetry: Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("svc".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            },
            upstreams: Vec::new(),
            routes: Vec::new(),
        };

        let validated = ValidatedRuntimeConfig::new(config.clone());
        match validated.as_ref().listeners[0].workers {
            WorkerCount::Count(count) => assert_eq!(count.get(), 2),
            WorkerCount::Auto => panic!("expected explicit worker count"),
        }
        assert_eq!(validated.telemetry.service_name.0, "svc");

        let inner = validated.into_inner();
        match inner.listeners[0].workers {
            WorkerCount::Count(count) => assert_eq!(count.get(), 2),
            WorkerCount::Auto => panic!("expected explicit worker count"),
        }
    }

    #[test]
    fn validated_runtime_from_trusted() {
        let config = RuntimeConfig {
            listeners: Vec::new(),
            telemetry: Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("svc".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            },
            upstreams: Vec::new(),
            routes: Vec::new(),
        };

        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(config.clone()) };
        assert_eq!(validated.listeners.len(), 0);
    }
}
