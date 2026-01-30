use crate::runtime::{
    RelayError, RelayMeta, RelayMetrics, RelayOptions, RelayRuntimeState, execute_plan,
};
use crate::storage::metadata::checksum_for_bytes;
use crate::storage::validated_path::ValidatedStorageRoot;
use axum::body::Bytes;
use pavis_core::{AccessLogPolicy, ListenerName, Metrics, ServiceName, Telemetry, WorkerCount};
use std::net::SocketAddr;
use std::path::PathBuf;

fn minimal_config() -> pavis_core::RuntimeConfig {
    let listener = pavis_core::ListenerBuilder::new()
        .name(ListenerName("default".to_string()))
        .address("127.0.0.1:8080".parse::<SocketAddr>().unwrap())
        .workers(WorkerCount::Auto)
        .tls(pavis_core::TlsConfig::Disabled)
        .build()
        .expect("listener");

    pavis_core::RuntimeConfigBuilder::new()
        .telemetry(Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName("pavis".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .build()
        .expect("config")
}

fn validated_config() -> pavis_core::ValidatedRuntimeConfig {
    pavis_core::validate_runtime(minimal_config()).expect("validate")
}

fn temp_storage_root(label: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.join(format!("relay_runtime_{label}_{pid}_{nanos}"))
}

fn state_with_storage(label: &str) -> RelayRuntimeState {
    let storage_root =
        ValidatedStorageRoot::new(temp_storage_root(label)).expect("validated storage root");
    let options = RelayOptions {
        storage_root,
        ..Default::default()
    };
    RelayRuntimeState::new_with_options(0, Bytes::new(), options).expect("state")
}

fn valid_pvs_bytes() -> Bytes {
    let validated = validated_config();
    Bytes::from(pavis_pvs::encode(validated.as_ref()).expect("encode"))
}
#[test]
fn execute_plan_rejects_non_monotonic_versions() {
    let err = execute_plan(5, 5).expect_err("non-monotonic");
    match err {
        RelayError::VersionMonotonicity { current, proposed } => {
            assert_eq!(current, 5);
            assert_eq!(proposed, 5);
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn state_tracks_version_and_snapshot() {
    let state = RelayRuntimeState::new(3, Bytes::new()).expect("state");
    assert_eq!(state.version().await, 3);
    let snapshot = state.snapshot().await;
    assert_eq!(snapshot.version, 3);
    assert!(snapshot.meta.checksum.is_empty());

    let meta = RelayMeta {
        checksum: "sum".to_string(),
        algorithm: "alg".to_string(),
        schema_version: 0,
    };
    state
        .publish(
            4,
            Bytes::from_static(b"bytes"),
            meta.clone(),
            checksum_for_bytes(b"bytes"),
        )
        .await
        .expect("publish");
    let snapshot = state.snapshot().await;
    assert_eq!(snapshot.version, 4);
    assert_eq!(snapshot.meta.checksum, "sum");
}

#[tokio::test]
async fn publish_config_accepts_validated_config() {
    let state = state_with_storage("publish_config");
    let validated = validated_config();
    let version = state
        .publish_config(&validated)
        .await
        .expect("publish_config");
    assert_eq!(version, 1);
    assert_eq!(state.version().await, 1);
}

#[tokio::test]
async fn state_publish_auto_increments_version() {
    // Use a valid PVS for publish_auto as it inspects the header
    let validated = validated_config();
    let pvs_bytes = pavis_pvs::encode(validated.as_ref()).expect("encode");

    let state = RelayRuntimeState::new(10, Bytes::new()).expect("state");
    assert_eq!(state.version().await, 10);

    let v11: u64 = state.publish_auto(pvs_bytes.into()).await.expect("publish");
    assert_eq!(v11, 11);
    assert_eq!(state.version().await, 11);
}

#[tokio::test]
async fn state_tracks_last_error() {
    let state = RelayRuntimeState::new(0, Bytes::new()).expect("state");
    assert!(state.last_error().await.is_none());
    state.set_last_error(Some("test error".to_string())).await;
    assert_eq!(state.last_error().await, Some("test error".to_string()));
}

#[tokio::test]
async fn state_tracks_generated_at() {
    let state = RelayRuntimeState::new(0, Bytes::new()).expect("state");
    let snapshot = state.snapshot().await;
    // Verify generated_at is recent (within 5s)
    let now = std::time::SystemTime::now();
    // Handle case where now is slightly before updated_at due to granularity
    let diff = now
        .duration_since(snapshot.updated_at)
        .unwrap_or(std::time::Duration::from_secs(0));
    assert!(diff.as_secs() < 5);

    let validated = validated_config();
    let pvs_bytes = pavis_pvs::encode(validated.as_ref()).expect("encode");
    let version = state.publish_auto(pvs_bytes.into()).await.expect("publish");

    let artifact = state.artifact(version).await.expect("artifact");
    // Re-fetch now
    let now = std::time::SystemTime::now();
    let diff = now
        .duration_since(artifact.generated_at)
        .unwrap_or(std::time::Duration::from_secs(0));
    assert!(diff.as_secs() < 5);
}

#[test]
fn relay_meta_empty_has_defaults() {
    let meta = RelayMeta::empty();
    assert!(meta.checksum.is_empty());
    assert!(meta.algorithm.is_empty());
    assert_eq!(meta.schema_version, 0);
}

#[tokio::test]
async fn state_returns_none_for_missing_artifact() {
    let state = RelayRuntimeState::new(0, Bytes::new()).expect("state");
    assert!(state.artifact(999).await.is_none());
}

#[tokio::test]
async fn state_publish_enforces_monotonicity() {
    let state = RelayRuntimeState::new(10, Bytes::new()).expect("state");
    let meta = RelayMeta::empty();

    // Same version
    let err = state
        .publish(
            10,
            Bytes::from_static(b"data"),
            meta.clone(),
            checksum_for_bytes(b"data"),
        )
        .await
        .expect_err("same version");

    match err {
        RelayError::VersionMonotonicity { current, proposed } => {
            assert_eq!(current, 10);
            assert_eq!(proposed, 10);
        }
        _ => panic!("expected monotonicity error"),
    }

    // Older version
    let err = state
        .publish(
            5,
            Bytes::from_static(b"data"),
            meta.clone(),
            checksum_for_bytes(b"data"),
        )
        .await
        .expect_err("older version");

    match err {
        RelayError::VersionMonotonicity { current, proposed } => {
            assert_eq!(current, 10);
            assert_eq!(proposed, 5);
        }
        _ => panic!("expected monotonicity error"),
    }

    // Newer version (ok)
    state
        .publish(
            11,
            Bytes::from_static(b"data"),
            meta,
            checksum_for_bytes(b"data"),
        )
        .await
        .expect("newer version");
    assert_eq!(state.version().await, 11);
}

#[tokio::test]
async fn publish_bytes_monotonic_versions() {
    let state = state_with_storage("publish_monotonic");
    let bytes = valid_pvs_bytes();

    let meta_v1 = state
        .publish_bytes(bytes.clone())
        .await
        .expect("publish v1");
    let meta_v2 = state
        .publish_bytes(bytes.clone())
        .await
        .expect("publish v2");

    assert_eq!(meta_v1.version, 1);
    assert_eq!(meta_v2.version, 2);
    assert_eq!(state.version().await, 2);
}

#[tokio::test]
async fn publish_bytes_idempotent_checksum() {
    let state = state_with_storage("publish_idempotent");
    let bytes = valid_pvs_bytes();

    let meta_v1 = state
        .publish_bytes(bytes.clone())
        .await
        .expect("publish v1");
    let meta_v2 = state
        .publish_bytes(bytes.clone())
        .await
        .expect("publish v2");

    assert_ne!(meta_v1.version, meta_v2.version);
    assert_eq!(meta_v1.checksum, meta_v2.checksum);
    assert_eq!(meta_v1.size, meta_v2.size);
}

#[tokio::test]
async fn publish_invalid_pvs_no_version_increment() {
    let state = state_with_storage("publish_invalid");
    let err = state
        .publish_bytes(Bytes::from_static(b"not a pvs"))
        .await
        .expect_err("invalid publish");

    match err {
        RelayError::Config(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }

    assert_eq!(state.version().await, 0);
}

#[tokio::test]
async fn publish_persists_state_json() {
    let state = state_with_storage("publish_state_json");
    let bytes = valid_pvs_bytes();
    let storage_root = &state.options().storage_root;

    let meta = state.publish_bytes(bytes).await.expect("publish");
    let state_path = storage_root.as_path().join("state.json");
    let loaded = crate::state::load_state(&state_path)
        .expect("load state")
        .expect("state exists");

    assert_eq!(loaded.current_version, meta.version);
}

#[tokio::test]
async fn publish_writes_history_and_lkg_metadata() {
    let state = state_with_storage("publish_history");
    let bytes = valid_pvs_bytes();
    let storage_root = &state.options().storage_root;

    let meta = state.publish_bytes(bytes).await.expect("publish");
    let history =
        crate::storage::history::list_history_versions(storage_root).expect("list history");
    assert_eq!(history, vec![meta.version]);

    let lkg_meta = crate::storage::lkg::load_lkg_metadata(storage_root)
        .expect("load lkg metadata")
        .expect("lkg metadata");
    assert_eq!(lkg_meta.version, meta.version);
    assert_eq!(lkg_meta.checksum, meta.checksum);
    assert_eq!(lkg_meta.size, meta.size);
}

#[tokio::test]
async fn publish_rejects_oversized_payload() {
    let storage_root = ValidatedStorageRoot::new(temp_storage_root("publish_oversize"))
        .expect("validated storage root");
    let mut options = RelayOptions {
        storage_root,
        ..Default::default()
    };
    options.max_pvs_bytes = 1;
    let state = RelayRuntimeState::new_with_options(0, Bytes::new(), options).expect("state");
    let bytes = valid_pvs_bytes();

    let err = state
        .publish_bytes(bytes)
        .await
        .expect_err("oversize publish");
    match err {
        RelayError::Policy(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(state.version().await, 0);
}

#[test]
fn test_metrics_counters() {
    let metrics = RelayMetrics::default();
    assert_eq!(metrics.publish_ok(), 0);
    assert_eq!(metrics.publish_fail(), 0);
    assert_eq!(metrics.long_poll_wait(), 0);

    metrics.inc_publish_ok();
    metrics.inc_publish_fail();
    metrics.inc_publish_fail();
    metrics.inc_long_poll_wait();

    assert_eq!(metrics.publish_ok(), 1);
    assert_eq!(metrics.publish_fail(), 2);
    assert_eq!(metrics.long_poll_wait(), 1);
}

#[tokio::test]
async fn test_enforce_limits() {
    let options = RelayOptions {
        max_pvs_bytes: 10,
        ..Default::default()
    };
    let state = RelayRuntimeState::new_with_options(0, Bytes::new(), options).expect("state");

    let big_bytes = Bytes::from(vec![0u8; 20]);
    let err = state
        .publish_auto(big_bytes)
        .await
        .expect_err("should fail");
    assert!(matches!(err, RelayError::Policy(_)));
    assert!(err.to_string().contains("exceeds max_pvs_bytes"));
}
