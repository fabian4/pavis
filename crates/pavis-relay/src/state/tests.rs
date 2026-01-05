#[cfg(test)]
mod tests {
    use crate::state::{
        RelayError, RelayMeta, RelayMetrics, RelayOptions, RelayState, execute_plan,
    };
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
        let config = minimal_config();
        let validated = pavis_core::validate_runtime(config).expect("validate");
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
        let config = minimal_config();
        let pvs_bytes = pavis_pvs::encode(&config).expect("encode");

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
        assert!(matches!(
            err,
            RelayError::VersionMonotonicity {
                current: 10,
                proposed: 10
            }
        ));

        // Older version
        let err = state
            .publish(5, Bytes::from_static(b"data"), meta.clone())
            .await
            .expect_err("older version");
        assert!(matches!(
            err,
            RelayError::VersionMonotonicity {
                current: 10,
                proposed: 5
            }
        ));

        // Newer version (ok)
        state
            .publish(11, Bytes::from_static(b"data"), meta)
            .await
            .expect("newer version");
        assert_eq!(state.version().await, 11);
    }

    #[tokio::test]
    async fn persistence_updates_last_error_on_failure() {
        let dir = std::env::temp_dir().join("relay_persist_fail");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Use a directory as the LKG path to force IsADirectory error on write
        let lkg = dir.join("config.pvs");
        std::fs::create_dir(&lkg).unwrap();

        let mut options = RelayOptions::default();
        options.persistence.enabled = true;
        options.persistence.flush_interval = std::time::Duration::from_millis(10);
        options.persistence.retry_max = 1; // fast fail
        options.persistence.retry_backoff = std::time::Duration::from_millis(1);
        options.lkg_path = Some(lkg.clone());

        // Use valid PVS bytes
        let config = minimal_config();
        let pvs_bytes = pavis_pvs::encode(&config).expect("encode");

        let state = RelayState::new_with_options(0, pvs_bytes.into(), options).expect("state");

        // Trigger persistence with valid bytes
        let update_bytes = pavis_pvs::encode(&config).expect("encode");
        let _ = state
            .publish_auto(update_bytes.into())
            .await
            .expect("publish");

        // Wait for persistence loop to fail
        let mut attempts = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if state.last_error().await.is_some() {
                break;
            }
            attempts += 1;
            if attempts > 20 {
                panic!("timed out waiting for persistence error");
            }
        }

        let err = state.last_error().await.unwrap();
        // Error will be "Is a directory" (Os { code: 21 })
        assert!(err.contains("Is a directory") || err.contains("Os { code: 21"));

        let _ = std::fs::remove_dir_all(&dir);
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
        let mut options = RelayOptions::default();
        options.max_pvs_bytes = 10;
        let state = RelayState::new_with_options(0, Bytes::new(), options).expect("state");

        let big_bytes = Bytes::from(vec![0u8; 20]);
        let err = state
            .publish_auto(big_bytes)
            .await
            .expect_err("should fail");
        assert!(matches!(err, RelayError::Policy(_)));
        assert!(err.to_string().contains("exceeds max_pvs_bytes"));
    }

    #[tokio::test]
    async fn test_persistence_success() {
        let dir = std::env::temp_dir().join("relay_persist_ok");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lkg = dir.join("config.pvs");

        let mut options = RelayOptions::default();
        options.persistence.enabled = true;
        options.persistence.flush_interval = std::time::Duration::from_millis(10);
        options.lkg_path = Some(lkg.clone());

        let state = RelayState::new_with_options(0, Bytes::new(), options).expect("state");

        // Publish something
        let mut config = minimal_config();
        config.telemetry.service_name = ServiceName("persist_test".to_string());
        let pvs_bytes = pavis_pvs::encode(&config).expect("encode");
        let _ = state
            .publish_auto(pvs_bytes.clone().into())
            .await
            .expect("publish");

        // Wait for file
        let mut attempts = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if lkg.exists() {
                let read = std::fs::read(&lkg).unwrap();
                if read == pvs_bytes {
                    break;
                }
            }
            attempts += 1;
            if attempts > 20 {
                panic!("timed out waiting for persistence");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_persistence_shutdown() {
        let dir = std::env::temp_dir().join("relay_persist_shutdown");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lkg = dir.join("config.pvs");

        let mut options = RelayOptions::default();
        options.persistence.enabled = true;
        // Long interval so it doesn't flush by timer
        options.persistence.flush_interval = std::time::Duration::from_secs(10);
        options.lkg_path = Some(lkg.clone());

        let state = RelayState::new_with_options(0, Bytes::new(), options).expect("state");

        let config = minimal_config();
        let pvs_bytes = pavis_pvs::encode(&config).expect("encode");
        let _ = state
            .publish_auto(pvs_bytes.clone().into())
            .await
            .expect("publish");

        // Drop state to trigger shutdown flush
        drop(state);

        // Give a little time for the background task to run its shutdown block
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert!(lkg.exists(), "File should exist after shutdown");
        let read = std::fs::read(&lkg).unwrap();
        assert_eq!(read, pvs_bytes);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
