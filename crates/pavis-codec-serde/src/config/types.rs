mod routes;
mod server;
mod telemetry;
mod upstreams;

pub use routes::{
    HeaderOperations, Matcher, RetryPolicy, RewritePolicy, Route, VirtualHost, WeightedDestination,
};
pub use server::{Listener, TlsConfig};
pub use telemetry::{TelemetryConfig, TracingConfig};
pub use upstreams::{
    CircuitBreaker, ConnectionPoolConfig, Endpoint, HealthCheck, Upstream, UpstreamTlsConfig,
};

use anyhow::Result as AnyResult;
use serde::{Deserialize, Serialize};

use pavis_core::RuntimeConfig;

use super::convert::structural;
use super::validation;
use crate::SerdeFormat;
use crate::serde_helpers::parse_with_format;

/// Source-format DTO parsed from JSON/YAML.
/// Optional fields remain sparse until structural completion.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SerdeConfig {
    pub listeners: Option<Vec<Listener>>,
    pub telemetry: Option<TelemetryConfig>,
    pub upstreams: Option<Vec<Upstream>>,
    pub routes: Option<Vec<VirtualHost>>,
}

/// Shape-complete DTO with containers present and explicit empty/disabled states.
/// This is still a codec-level structure and is not core-validated.
#[derive(Debug, Clone)]
pub struct StructurallyConfig {
    pub listeners: Vec<Listener>,
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

    /// Parse, validate, structurally complete, and convert into a `RuntimeConfig`.
    /// Core semantic validation is handled by the codec pipeline, not here.
    pub fn build(self) -> AnyResult<RuntimeConfig> {
        let mut config = self;
        validation::validate(&mut config)?;
        let complete = structural(config);
        complete.try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::SerdeConfig;
    use crate::SerdeFormat;

    fn assert_sparse(config: SerdeConfig) {
        let upstream = &config.upstreams.as_ref().unwrap()[0];
        assert_eq!(upstream.balancer, None);
        assert_eq!(upstream.protocol, None);
        assert!(upstream.pool.is_none());
        let tls = upstream.tls.as_ref().expect("tls config");
        assert_eq!(tls.enabled, None);
        assert_eq!(config.telemetry.as_ref().unwrap().access_log, None);
    }

    #[test]
    fn parse_leaves_upstream_and_telemetry_sparse() {
        let yaml = r#"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
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
      - matcher: !prefix
          path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect("parse yaml");
        assert_sparse(config);
    }

    #[test]
    fn parse_leaves_upstream_and_telemetry_sparse_json() {
        let json = r#"
{
  "listeners": [{ "name": "default", "address": "0.0.0.0:8080" }],
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
          "matcher": { "prefix": { "path": "/" } },
          "destinations": [{ "upstream": "backend", "weight": 1 }]
        }
      ]
    }
  ]
}
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Json, json).expect("parse json");
        assert_sparse(config);
    }

    #[test]
    fn parse_rejects_invalid_duration() {
        let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    pool:
      idle: "not-a-duration"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes: []
"#;

        let err = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect_err("invalid duration");
        assert!(err.to_string().contains("idle"));
    }

    #[test]
    fn parse_bytes_accepts_json() {
        let json = br#"
{
  "listeners": [{ "name": "default", "address": "0.0.0.0:8080" }],
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
        assert_eq!(
            config.listeners.as_ref().unwrap()[0].address,
            "0.0.0.0:8080"
        );
    }
}
