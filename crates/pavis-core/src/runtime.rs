mod headers;
mod routing;
mod server;
mod telemetry;
mod upstream;

pub use headers::HeaderOperations;
pub use routing::{MatchType, RetryPolicy, Route, VirtualHost, WeightedDestination};
pub use server::{ServerConfig, TlsConfig};
pub use telemetry::{AccessLogConfig, LogLevel, TelemetryConfig, TracingConfig};
pub use upstream::{
    ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, Upstream, UpstreamTlsConfig,
};

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

/// The Root Configuration Object.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct RuntimeConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
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

    #[test]
    fn test_config_structure() {
        let config = RuntimeConfig {
            server: ServerConfig {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080),
                worker_threads: Some(4),
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: Some(LogLevel::Info),
                pingora: None,
                service_name: Some("test".to_string()),
                prometheus_addr: Some("0.0.0.0:9090".to_string()),
                access_log: AccessLogConfig::Stdout,
                tracing: None,
            },
            upstreams: vec![Upstream {
                name: "upstream1".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H1,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 60,
                    connection_timeout_secs: 5,
                },
                tls: None,
                endpoints: vec![Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: 8080,
                    weight: 1,
                }],
            }],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Prefix,
                    path: "/".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "upstream1".to_string(),
                        weight: 1,
                    }],
                }],
            }],
        };

        assert_eq!(config.server.worker_threads, Some(4));
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.routes.len(), 1);
    }
}
