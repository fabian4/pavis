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

use anyhow::Result as AnyResult;
use serde::{Deserialize, Serialize};

use pavis_core::RuntimeConfig;

use super::validation;
use crate::SerdeFormat;
use crate::serde_helpers::parse_with_format;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SerdeConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

impl SerdeConfig {
    pub fn parse_str(format: SerdeFormat, content: &str) -> AnyResult<Self> {
        parse_with_format(format, content.as_bytes())
    }

    pub fn parse_bytes(format: SerdeFormat, bytes: &[u8]) -> AnyResult<Self> {
        parse_with_format(format, bytes)
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
    use super::SerdeConfig;
    use crate::SerdeFormat;
    use pavis_core::{AccessLogConfig, HttpVersion, LoadBalancer};
    use std::time::Duration;

    fn assert_defaults(config: SerdeConfig) {
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

        let config = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect("parse yaml");
        assert_defaults(config);
    }

    #[test]
    fn parse_applies_defaults_for_upstream_and_telemetry_json() {
        let json = r#"
{
  "server": { "listen_addr": "0.0.0.0:8080" },
  "telemetry": {},
  "upstreams": [
    {
      "name": "backend",
      "tls": {},
      "endpoints": [{ "ip": "127.0.0.1", "port": 8081 }]
    }
  ],
  "routes": [
    {
      "host": "example.com",
      "paths": [
        {
          "path": "/",
          "destinations": [{ "upstream": "backend", "weight": 1 }]
        }
      ]
    }
  ]
}
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Json, json).expect("parse json");
        assert_defaults(config);
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

        let err = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect_err("invalid duration");
        assert!(err.to_string().contains("idle_timeout"));
    }

    #[test]
    fn parse_bytes_accepts_json() {
        let json = br#"
{
  "server": { "listen_addr": "0.0.0.0:8080" },
  "telemetry": {},
  "upstreams": [
    {
      "name": "backend",
      "tls": {},
      "endpoints": [{ "ip": "127.0.0.1", "port": 8081 }]
    }
  ],
  "routes": []
}
"#;
        let config = SerdeConfig::parse_bytes(SerdeFormat::Json, json).expect("parse bytes");
        assert_eq!(config.server.listen_addr, "0.0.0.0:8080");
    }
}
