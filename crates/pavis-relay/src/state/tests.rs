use crate::state::{RelayError, RelayMeta, RelayMetrics, RelayOptions, RelayState, execute_plan};
use axum::body::Bytes;
use pavis_core::{AccessLogPolicy, ListenerName, Metrics, ServiceName, Telemetry, WorkerCount};
use std::net::SocketAddr;

fn minimal_config() -> pavis_core::RuntimeConfig {
    pavis_core::RuntimeConfig {
        listeners: vec![pavis_core::Listener {
            name: ListenerName("default".to_string()),
            address: "127.0.0.1:8080".parse::<SocketAddr>().unwrap(),
            workers: WorkerCount::Auto,
            tls: pavis_core::TlsConfig::Disabled,
        }],
        telemetry: Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName("pavis".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        },
        upstreams: vec![],
        routes: vec![],
    }
}

fn validated_config() -> pavis_core::ValidatedRuntimeConfig {
    pavis_core::validate_runtime(minimal_config()).expect("validate")
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
    let state = RelayState::new(3, Bytes::new()).expect("state");
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
        .publish(4, Bytes::from_static(b"bytes"), meta.clone())
        .await
        .expect("publish");
    let snapshot = state.snapshot().await;
    assert_eq!(snapshot.version, 4);
    assert_eq!(snapshot.meta.checksum, "sum");
}

#[tokio::test]
async fn publish_config_accepts_validated_config() {
    let state = RelayState::new(0, Bytes::new()).expect("state");
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

    let state = RelayState::new(10, Bytes::new()).expect("state");
    assert_eq!(state.version().await, 10);

    let v11: u64 = state.publish_auto(pvs_bytes.into()).await.expect("publish");
    assert_eq!(v11, 11);
    assert_eq!(state.version().await, 11);
}

#[tokio::test]
async fn state_tracks_last_error() {
    let state = RelayState::new(0, Bytes::new()).expect("state");
    assert!(state.last_error().await.is_none());
    state.set_last_error(Some("test error".to_string())).await;
    assert_eq!(state.last_error().await, Some("test error".to_string()));
}

#[tokio::test]
async fn state_tracks_generated_at() {
    let state = RelayState::new(0, Bytes::new()).expect("state");
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
    let state = RelayState::new(0, Bytes::new()).expect("state");
    assert!(state.artifact(999).await.is_none());
}

#[tokio::test]
async fn state_publish_enforces_monotonicity() {
    let state = RelayState::new(10, Bytes::new()).expect("state");
    let meta = RelayMeta::empty();

    // Same version
    let err = state
        .publish(10, Bytes::from_static(b"data"), meta.clone())
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
        .publish(5, Bytes::from_static(b"data"), meta.clone())
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
        .publish(11, Bytes::from_static(b"data"), meta)
        .await
        .expect("newer version");
    assert_eq!(state.version().await, 11);
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
    let state = RelayState::new_with_options(0, Bytes::new(), options).expect("state");

    let big_bytes = Bytes::from(vec![0u8; 20]);
    let err = state
        .publish_auto(big_bytes)
        .await
        .expect_err("should fail");
    assert!(matches!(err, RelayError::Policy(_)));
    assert!(err.to_string().contains("exceeds max_pvs_bytes"));
}
