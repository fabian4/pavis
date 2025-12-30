use super::RelayConfig;
use super::env::expand_env;
use super::load::decode_value;

fn decode_str_with_env<F>(input: &str, lookup: F) -> anyhow::Result<RelayConfig>
where
    F: Fn(&str) -> Result<String, std::env::VarError>,
{
    let mut value: serde_yaml::Value = serde_yaml::from_str(input)?;
    expand_env(&mut value, &lookup)?;
    decode_value(value)
}

fn decode_str(input: &str) -> anyhow::Result<RelayConfig> {
    decode_str_with_env(input, |k| std::env::var(k))
}

#[test]
fn load_accepts_root_relay_key_and_nested_security() -> anyhow::Result<()> {
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
fn load_accepts_flat_root_and_optional_admin_bind() -> anyhow::Result<()> {
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
fn load_expands_environment_variables() -> anyhow::Result<()> {
    let lookup = |key: &str| -> Result<String, std::env::VarError> {
        if key == "PAVIS_RELAY_TEST" {
            Ok("dev".to_string())
        } else {
            Err(std::env::VarError::NotPresent)
        }
    };

    let config = decode_str_with_env(
        r#"
relay:
  identity:
    name: pavis-relay
    cluster: "${PAVIS_RELAY_TEST}"
    instance_id: "localhost"
  http:
    bind: "127.0.0.1:8081"
"#,
        lookup,
    )?;

    assert_eq!(config.identity.cluster, "dev");
    Ok(())
}

#[test]
fn load_accepts_minimal_config() -> anyhow::Result<()> {
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
fn load_populates_full_config_fields() -> anyhow::Result<()> {
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
