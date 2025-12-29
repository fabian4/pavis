#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
pub struct RelayConfig {
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub http: HttpConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub artifact: ArtifactConfig,
    #[serde(default)]
    pub pipeline: PipelineConfig,
    #[serde(default)]
    pub distribution: DistributionConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct IdentityConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cluster: String,
    #[serde(default)]
    pub instance_id: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct HttpConfig {
    #[serde(default)]
    pub bind: String,
    #[serde(default)]
    pub admin_bind: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    #[serde(default)]
    pub storage_type: String,
    #[serde(default)]
    pub root_dir: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ArtifactConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pvs_filename: String,
    #[serde(default)]
    pub lkg_path: String,
    #[serde(default)]
    pub artifacts_dir: String,
    #[serde(default)]
    pub limits: ArtifactLimits,
}

#[derive(Debug, Deserialize, Default)]
pub struct ArtifactLimits {
    #[serde(default)]
    pub max_pvs_bytes: u64,
    #[serde(default)]
    pub max_routes: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub ingest: IngestConfig,
    #[serde(default)]
    pub codec: CodecConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct IngestConfig {
    #[serde(default)]
    pub source: IngestSource,
    #[serde(default)]
    pub mode: IngestMode,
}

#[derive(Debug, Deserialize, Default)]
pub struct IngestSource {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub config: serde_yaml::Value,
}

#[derive(Debug, Deserialize, Default)]
pub struct IngestMode {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub config: serde_yaml::Value,
}

#[derive(Debug, Deserialize, Default)]
pub struct CodecConfig {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub options: CodecOptions,
}

#[derive(Debug, Deserialize, Default)]
pub struct CodecOptions {
    #[serde(default)]
    pub strict_unknown_fields: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct ExecutionConfig {
    #[serde(default)]
    pub versioning: VersioningConfig,
    #[serde(default)]
    pub publish: PublishConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct VersioningConfig {
    #[serde(default)]
    pub scheme: String,
    #[serde(default)]
    pub state_file: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct PublishConfig {
    #[serde(default)]
    pub atomic_write: bool,
    #[serde(default)]
    pub fsync: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct DistributionConfig {
    #[serde(default)]
    pub long_poll: LongPollConfig,
    #[serde(default)]
    pub direct_fetch: DirectFetchConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct LongPollConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub headers: LongPollHeaders,
    #[serde(default)]
    pub timeouts: LongPollTimeouts,
}

#[derive(Debug, Deserialize, Default)]
pub struct LongPollHeaders {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub checksum: String,
    #[serde(default)]
    pub algorithm: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct LongPollTimeouts {
    #[serde(default)]
    pub hold_seconds: u64,
    #[serde(default)]
    pub idle_seconds: u64,
}

#[derive(Debug, Deserialize, Default)]
pub struct DirectFetchConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub bearer: BearerConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct BearerConfig {
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct LoggingConfig {
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub access_log: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct MetricsConfig {
    #[serde(default)]
    pub prometheus_bind: String,
}

pub fn load(path: &Path) -> Result<RelayConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read relay config: {}", path.display()))?;
    let mut value: serde_yaml::Value = serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse relay config: {}", path.display()))?;
    expand_env(&mut value).context("failed to expand environment variables")?;
    decode_value(value)
}

fn normalize_root(mut value: serde_yaml::Value) -> Result<serde_yaml::Value> {
    let relay_key = serde_yaml::Value::String("relay".to_string());
    let distribution_key = serde_yaml::Value::String("distribution".to_string());
    let security_key = serde_yaml::Value::String("security".to_string());

    if let serde_yaml::Value::Mapping(map) = &mut value
        && let Some(relay_value) = map.remove(&relay_key)
    {
        value = relay_value;
    }

    if let serde_yaml::Value::Mapping(map) = &mut value
        && !map.contains_key(&security_key)
    {
        let nested_security =
            map.get_mut(&distribution_key)
                .and_then(|distribution| match distribution {
                    serde_yaml::Value::Mapping(dist_map) => dist_map.remove(&security_key),
                    _ => None,
                });
        if let Some(security) = nested_security {
            map.insert(security_key, security);
        }
    }

    Ok(value)
}

fn decode_value(value: serde_yaml::Value) -> Result<RelayConfig> {
    let value = normalize_root(value)?;
    serde_yaml::from_value(value).context("failed to decode relay config")
}

fn expand_env(value: &mut serde_yaml::Value) -> Result<()> {
    match value {
        serde_yaml::Value::String(s) => {
            *s = expand_env_str(s)?;
        }
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                expand_env(item)?;
            }
        }
        serde_yaml::Value::Mapping(map) => {
            for (_, value) in map.iter_mut() {
                expand_env(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn expand_env_str(input: &str) -> Result<String> {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let remaining = &rest[start + 2..];
        let Some(end) = remaining.find('}') else {
            return Err(anyhow::anyhow!(
                "unterminated environment variable reference"
            ));
        };
        let key = &remaining[..end];
        let value =
            std::env::var(key).with_context(|| format!("missing environment variable: {key}"))?;
        out.push_str(&value);
        rest = &remaining[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_str(input: &str) -> Result<RelayConfig> {
        let mut value: serde_yaml::Value = serde_yaml::from_str(input)?;
        expand_env(&mut value)?;
        decode_value(value)
    }

    #[test]
    fn load_accepts_root_relay_key_and_nested_security() -> Result<()> {
        let config = decode_str(
            r#"
relay:
  identity:
    name: pavis-relay
    cluster: dev
    instance_id: "localhost"
  http:
    bind: "0.0.0.0:8080"
  storage:
    type: filesystem
    root_dir: "/var/lib/pavis"
  artifact:
    name: "default"
    pvs_filename: "config.pvs"
    lkg_path: "/var/lib/pavis/lkg/config.pvs"
    artifacts_dir: "/var/lib/pavis/artifacts"
    limits:
      max_pvs_bytes: 1
  pipeline:
    source_id: "static:dev-config"
    ingest:
      source:
        kind: static
        config:
          path: "/etc/pavis/input.yaml"
      mode:
        kind: watch
        config:
          debounce_ms: 200
    codec:
      kind: yaml
      options:
        strict_unknown_fields: true
    execution:
      versioning:
        scheme: monotonic_u64
        state_file: "/var/lib/pavis/state.json"
      publish:
        atomic_write: true
        fsync: true
  distribution:
    long_poll:
      enabled: true
      headers:
        version: "X-Pavis-Version"
        checksum: "X-Pavis-Checksum"
      timeouts:
        hold_seconds: 1
        idle_seconds: 1
    direct_fetch:
      enabled: true
    security:
      auth:
        mode: none
        bearer:
          token: ""
  logging:
    level: info
    access_log: true
  metrics:
    prometheus_bind: "0.0.0.0:9100"
"#,
        )?;

        assert_eq!(config.identity.name, "pavis-relay");
        assert_eq!(config.http.bind, "0.0.0.0:8080");
        assert_eq!(config.security.auth.mode, "none");
        Ok(())
    }

    #[test]
    fn load_accepts_flat_root_and_optional_admin_bind() -> Result<()> {
        let config = decode_str(
            r#"
identity:
  name: pavis-relay
  cluster: dev
  instance_id: "localhost"
http:
  bind: "127.0.0.1:8081"
storage:
  type: filesystem
  root_dir: "/var/lib/pavis"
artifact:
  name: "default"
  pvs_filename: "config.pvs"
  lkg_path: "/var/lib/pavis/lkg/config.pvs"
  artifacts_dir: "/var/lib/pavis/artifacts"
  limits:
    max_pvs_bytes: 1
  pipeline:
    source_id: "static:dev-config"
    ingest:
      source:
        kind: static
        config:
          path: "/etc/pavis/input.yaml"
      mode:
        kind: watch
        config:
          debounce_ms: 200
    codec:
      kind: yaml
      options:
        strict_unknown_fields: true
    execution:
      versioning:
        scheme: monotonic_u64
        state_file: "/var/lib/pavis/state.json"
      publish:
        atomic_write: true
        fsync: true
distribution:
  long_poll:
    enabled: true
    headers:
      version: "X-Pavis-Version"
      checksum: "X-Pavis-Checksum"
    timeouts:
      hold_seconds: 1
      idle_seconds: 1
  direct_fetch:
    enabled: true
security:
  auth:
    mode: none
    bearer:
      token: ""
logging:
  level: info
  access_log: true
metrics:
  prometheus_bind: "0.0.0.0:9100"
"#,
        )?;

        assert_eq!(config.http.admin_bind, None);
        assert_eq!(config.http.bind, "127.0.0.1:8081");
        Ok(())
    }

    #[test]
    fn load_expands_environment_variables() -> Result<()> {
        unsafe {
            std::env::set_var("PAVIS_RELAY_TEST", "dev");
        }
        let config = decode_str(
            r#"
identity:
  name: pavis-relay
  cluster: "${PAVIS_RELAY_TEST}"
  instance_id: "localhost"
http:
  bind: "127.0.0.1:8081"
storage:
  type: filesystem
  root_dir: "/var/lib/pavis"
artifact:
  name: "default"
  pvs_filename: "config.pvs"
  lkg_path: "/var/lib/pavis/lkg/config.pvs"
  artifacts_dir: "/var/lib/pavis/artifacts"
  limits:
    max_pvs_bytes: 1
  pipeline:
    source_id: "static:dev-config"
    ingest:
      source:
        kind: static
        config:
          path: "/etc/pavis/input.yaml"
      mode:
        kind: watch
        config:
          debounce_ms: 200
    codec:
      kind: yaml
      options:
        strict_unknown_fields: true
    execution:
      versioning:
        scheme: monotonic_u64
        state_file: "/var/lib/pavis/state.json"
      publish:
        atomic_write: true
        fsync: true
distribution:
  long_poll:
    enabled: true
    headers:
      version: "X-Pavis-Version"
      checksum: "X-Pavis-Checksum"
    timeouts:
      hold_seconds: 1
      idle_seconds: 1
  direct_fetch:
    enabled: true
security:
  auth:
    mode: none
    bearer:
      token: ""
logging:
  level: info
  access_log: true
metrics:
  prometheus_bind: "0.0.0.0:9100"
"#,
        )?;

        assert_eq!(config.identity.cluster, "dev");
        Ok(())
    }

    #[test]
    fn load_accepts_minimal_config() -> Result<()> {
        let config = decode_str(
            r#"
http:
  bind: "127.0.0.1:8080"
artifact:
  lkg_path: "/var/lib/pavis/lkg/config.pvs"
"#,
        )?;

        assert_eq!(config.http.bind, "127.0.0.1:8080");
        assert_eq!(config.artifact.lkg_path, "/var/lib/pavis/lkg/config.pvs");
        Ok(())
    }

    #[test]
    fn load_populates_full_config_fields() -> Result<()> {
        let config = decode_str(
            r#"
identity:
  name: pavis-relay
  cluster: prod
  instance_id: relay-1
http:
  bind: "127.0.0.1:8080"
  admin_bind: "127.0.0.1:9090"
storage:
  type: filesystem
  root_dir: "/var/lib/pavis"
artifact:
  name: "default"
  pvs_filename: "config.pvs"
  lkg_path: "/var/lib/pavis/lkg/config.pvs"
  artifacts_dir: "/var/lib/pavis/artifacts"
  limits:
    max_pvs_bytes: 1024
    max_routes: 200
pipeline:
  source_id: "static:dev"
  ingest:
    source:
      kind: static
      config:
        path: "/etc/pavis/input.yaml"
    mode:
      kind: watch
      config:
        debounce_ms: 200
  codec:
    kind: yaml
    options:
      strict_unknown_fields: true
  execution:
    versioning:
      scheme: monotonic_u64
      state_file: "/var/lib/pavis/state.json"
    publish:
      atomic_write: true
      fsync: true
distribution:
  long_poll:
    enabled: true
    headers:
      version: "X-Pavis-Version"
      checksum: "X-Pavis-Checksum"
      algorithm: "X-Pavis-Checksum-Alg"
    timeouts:
      hold_seconds: 55
      idle_seconds: 60
  direct_fetch:
    enabled: true
security:
  auth:
    mode: bearer
    bearer:
      token: "token"
logging:
  level: debug
  access_log: false
metrics:
  prometheus_bind: "0.0.0.0:9100"
"#,
        )?;

        assert_eq!(config.identity.cluster, "prod");
        assert_eq!(config.identity.instance_id, "relay-1");
        assert_eq!(config.http.admin_bind.as_deref(), Some("127.0.0.1:9090"));
        assert_eq!(config.storage.storage_type, "filesystem");
        assert_eq!(config.storage.root_dir, "/var/lib/pavis");
        assert_eq!(config.artifact.name, "default");
        assert_eq!(config.artifact.pvs_filename, "config.pvs");
        assert_eq!(config.artifact.artifacts_dir, "/var/lib/pavis/artifacts");
        assert_eq!(config.artifact.limits.max_pvs_bytes, 1024);
        assert_eq!(config.artifact.limits.max_routes, Some(200));
        assert_eq!(config.pipeline.source_id, "static:dev");
        assert_eq!(config.pipeline.ingest.source.kind, "static");
        assert_eq!(
            config.pipeline.ingest.source.config["path"],
            "/etc/pavis/input.yaml"
        );
        assert_eq!(config.pipeline.ingest.mode.kind, "watch");
        assert_eq!(config.pipeline.ingest.mode.config["debounce_ms"], 200);
        assert_eq!(config.pipeline.codec.kind, "yaml");
        assert!(config.pipeline.codec.options.strict_unknown_fields);
        assert_eq!(config.pipeline.execution.versioning.scheme, "monotonic_u64");
        assert_eq!(
            config.pipeline.execution.versioning.state_file,
            "/var/lib/pavis/state.json"
        );
        assert!(config.pipeline.execution.publish.atomic_write);
        assert!(config.pipeline.execution.publish.fsync);
        assert!(config.distribution.long_poll.enabled);
        assert_eq!(
            config.distribution.long_poll.headers.version,
            "X-Pavis-Version"
        );
        assert_eq!(
            config.distribution.long_poll.headers.checksum,
            "X-Pavis-Checksum"
        );
        assert_eq!(
            config.distribution.long_poll.headers.algorithm.as_deref(),
            Some("X-Pavis-Checksum-Alg")
        );
        assert_eq!(config.distribution.long_poll.timeouts.hold_seconds, 55);
        assert_eq!(config.distribution.long_poll.timeouts.idle_seconds, 60);
        assert!(config.distribution.direct_fetch.enabled);
        assert_eq!(config.security.auth.mode, "bearer");
        assert_eq!(config.security.auth.bearer.token, "token");
        assert_eq!(config.logging.level, "debug");
        assert!(!config.logging.access_log);
        assert_eq!(config.metrics.prometheus_bind, "0.0.0.0:9100");
        Ok(())
    }
}
