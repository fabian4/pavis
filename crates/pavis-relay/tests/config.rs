use pavis_relay::config;
use std::fs;
use std::path::PathBuf;

fn write_config(contents: &str) -> anyhow::Result<PathBuf> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("pavis_relay_config_{pid}_{id}.yaml"));
    fs::write(&path, contents)?;
    Ok(path)
}

#[test]
fn load_accepts_root_relay_key_and_nested_security() -> anyhow::Result<()> {
    let path = write_config(
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
    source_id: "file:dev-config"
    ingest:
      source:
        kind: file
        path: "/etc/pavis/input.yaml"
      mode:
        kind: watch
        debounce: 200
    codec:
      kind: serde
      mode:
        compaction: off
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

    let config = config::load(&path)?;
    let _ = fs::remove_file(&path);

    assert_eq!(config.identity.name, "pavis-relay");
    assert_eq!(config.http.bind, "0.0.0.0:8080");
    assert_eq!(config.security.auth.mode, "none");
    Ok(())
}

#[test]
fn load_accepts_flat_root_and_optional_admin_bind() -> anyhow::Result<()> {
    let path = write_config(
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
  source_id: "file:dev-config"
  ingest:
    source:
      kind: file
      path: "/etc/pavis/input.yaml"
    mode:
      kind: watch
      debounce: 200
  codec:
    kind: serde
    mode:
      compaction: off
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

    let config = config::load(&path)?;
    let _ = fs::remove_file(&path);

    assert_eq!(config.http.admin_bind, None);
    assert_eq!(config.http.bind, "127.0.0.1:8081");
    Ok(())
}

#[test]
fn load_expands_environment_variables() -> anyhow::Result<()> {
    let mut env_iter = std::env::vars().filter(|(_, value)| !value.is_empty());
    let Some((key, value)) = env_iter.next() else {
        return Ok(());
    };

    let path = write_config(&format!(
        r#"
relay:
  identity:
    name: pavis-relay
    cluster: "${{{key}}}"
    instance_id: "localhost"
  http:
    bind: "127.0.0.1:8081"
"#
    ))?;

    let config = config::load(&path)?;
    let _ = fs::remove_file(&path);

    assert_eq!(config.identity.cluster, value);
    Ok(())
}

#[test]
fn load_accepts_minimal_config() -> anyhow::Result<()> {
    let path = write_config(
        r#"
http:
  bind: "127.0.0.1:8080"
artifact:
  lkg_path: "/var/lib/pavis/lkg/config.pvs"
"#,
    )?;

    let config = config::load(&path)?;
    let _ = fs::remove_file(&path);

    assert_eq!(config.http.bind, "127.0.0.1:8080");
    assert_eq!(config.artifact.lkg_path, "/var/lib/pavis/lkg/config.pvs");
    Ok(())
}

#[test]
fn load_populates_full_config_fields() -> anyhow::Result<()> {
    let path = write_config(
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
  source_id: "file:dev"
  ingest:
    source:
      kind: file
      path: "/etc/pavis/input.yaml"
    mode:
      kind: watch
      debounce: 200
  codec:
    kind: serde
    mode:
      compaction: off
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

    let config = config::load(&path)?;
    let _ = fs::remove_file(&path);

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
    assert_eq!(config.pipeline.source_id, "file:dev");

    let config::IngestSource::File(file_cfg) = config.pipeline.ingest.source;
    assert_eq!(file_cfg.path, "/etc/pavis/input.yaml");

    assert_eq!(config.pipeline.ingest.mode.kind, "watch");
    assert_eq!(config.pipeline.ingest.mode.debounce, 200);

    assert!(matches!(
        config.pipeline.codec.kind,
        config::CodecKind::Serde
    ));

    assert_eq!(config.pipeline.execution.versioning.scheme, "monotonic_u64");
    assert_eq!(
        config.pipeline.execution.versioning.state_file,
        "/var/lib/pavis/state.json"
    );
    assert!(config.pipeline.execution.publish.atomic_write);
    assert!(config.pipeline.execution.publish.fsync);
    assert!(config.distribution.long_poll.enabled);
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
