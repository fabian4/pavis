mod routes;
mod server;
mod telemetry;
mod upstreams;

pub use routes::{HeaderOperations, RetryPolicy, Route, VirtualHost, WeightedDestination};
pub use server::{ServerConfig, TlsConfig};
pub use telemetry::{TelemetryConfig, TracingConfig};
pub use upstreams::{
    CircuitBreaker, ConnectionPoolConfig, Endpoint, HealthCheck, Upstream, UpstreamTlsConfig,
};

use anyhow::{Context, Result as AnyResult};
use serde::{Deserialize, Serialize};
use std::str;

use pavis_core::RuntimeConfig;

use super::validation;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct YamlConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

impl YamlConfig {
    pub fn parse_str(content: &str) -> AnyResult<Self> {
        serde_yaml::from_str(content).map_err(Into::into)
    }

    pub fn parse_bytes(bytes: &[u8]) -> AnyResult<Self> {
        let content = str::from_utf8(bytes).context("Config bytes must be UTF-8")?;
        Self::parse_str(content)
    }

    pub fn validate(&mut self) -> AnyResult<()> {
        validation::validate(self)
    }

    pub fn build(self) -> AnyResult<RuntimeConfig> {
        let mut config = self;
        validation::validate(&mut config)?;
        config.try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::YamlConfig;
    use pavis_core::{AccessLogConfig, HttpVersion, LoadBalancer};
    use std::time::Duration;

    #[test]
    fn parse_applies_defaults_for_upstream_and_telemetry() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    tls: {}
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
"#;

        let config = YamlConfig::parse_str(yaml).expect("parse yaml");
        let upstream = &config.upstreams[0];
        assert_eq!(upstream.load_balancer, LoadBalancer::Random);
        assert_eq!(upstream.http_version, HttpVersion::H1);
        assert_eq!(
            upstream.connection_pool.idle_timeout,
            Duration::from_secs(60)
        );
        assert_eq!(
            upstream.connection_pool.connection_timeout,
            Duration::from_secs(5)
        );
        let tls = upstream.tls.as_ref().expect("tls config");
        assert!(tls.enabled);
        assert_eq!(config.telemetry.access_log, AccessLogConfig::Stdout);
    }

    #[test]
    fn parse_rejects_invalid_duration() {
        let yaml = r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    connection_pool:
      idle_timeout: "not-a-duration"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes: []
"#;

        let err = YamlConfig::parse_str(yaml).expect_err("invalid duration");
        assert!(err.to_string().contains("idle_timeout"));
    }
}
