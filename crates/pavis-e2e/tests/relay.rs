use anyhow::Result;
use pavis_core::{AccessLogConfig, RuntimeConfig, ServerConfig, TelemetryConfig};
use pavis_e2e::support::relay::{RelayInstance, RelayOptions};
use pavis_pvs::PAVIS_MAGIC;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tokio::time::sleep;

fn default_runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        server: ServerConfig {
            listen_addr: "127.0.0.1:8080".parse().unwrap(),
            worker_threads: None,
            tls: None,
        },
        telemetry: TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: AccessLogConfig::Disabled,
            tracing: None,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    }
}

// R1: Publish increments artifact version and updates LKG atomically
#[tokio::test]
async fn r1_publish_increments_version_and_updates_lkg() -> Result<()> {
    let relay: RelayInstance = RelayInstance::new(RelayOptions::default()).await?;
    let client = relay.client();

    // Initial state
    let status = client.status().await?;
    assert_eq!(status.version, 0);

    // Publish v1
    let config_v1 = default_runtime_config();
    let resp = client.publish(&config_v1).await?;
    assert_eq!(resp.version, 1);
    assert!(!resp.checksum.is_empty());

    // Verify v1 artifact
    let artifact: Vec<u8> = client.get_artifact(1).await?;
    assert!(artifact.starts_with(PAVIS_MAGIC));

    // Verify status update
    let status = client.status().await?;
    assert_eq!(status.version, 1);
    assert_eq!(status.checksum, resp.checksum);

    // Verify LKG on disk
    let lkg_bytes = fs::read(&relay.lkg_path)?;
    assert!(lkg_bytes.starts_with(PAVIS_MAGIC));

    // Publish v2
    let mut config_v2 = default_runtime_config();
    config_v2.server.listen_addr = "127.0.0.1:9090".parse().unwrap();
    let resp_v2 = client.publish(&config_v2).await?;
    assert_eq!(resp_v2.version, 2);

    // Verify LKG updated
    let status_v2 = client.status().await?;
    assert_eq!(status_v2.version, 2);
    assert_ne!(status_v2.checksum, resp.checksum);

    Ok(())
}

// R2: Reject invalid .pvs
#[tokio::test]
async fn r2_reject_invalid_pvs() -> Result<()> {
    let relay: RelayInstance = RelayInstance::new(RelayOptions::default()).await?;
    let client = relay.client();

    let invalid_bytes = b"PAVS\x00\x00\x00\x01INVALID";
    let err = client
        .publish_raw(invalid_bytes.to_vec())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("422"));

    let status = client.status().await?;
    assert_eq!(status.version, 0);

    Ok(())
}

// R3: Long-poll semantics
#[tokio::test]
async fn r3_long_poll_semantics() -> Result<()> {
    let relay: RelayInstance = RelayInstance::new(RelayOptions::default()).await?;
    let client = relay.client();

    // Publish v1
    client.publish(&default_runtime_config()).await?;

    // Wait for timeout (304)
    let start = std::time::Instant::now();
    let poll_res: Option<(u64, Vec<u8>)> = client.long_poll(1, 1000).await?;
    assert!(poll_res.is_none()); // 304
    assert!(start.elapsed() >= Duration::from_millis(1000));

    // Wait for update
    let base_url = relay.env.base_url().to_string();

    let handle = tokio::spawn(async move {
        sleep(Duration::from_millis(500)).await;
        let mut cfg = default_runtime_config();
        cfg.server.listen_addr = "127.0.0.1:9091".parse().unwrap();
        // Manual client since we can't move relay
        let client = pavis_e2e::support::relay::RelayClient::new(base_url);
        client.publish(&cfg).await
    });

    let poll_res: Option<(u64, Vec<u8>)> = client.long_poll(1, 2000).await?;
    assert!(poll_res.is_some());
    let (version, _) = poll_res.unwrap();
    assert_eq!(version, 2);

    handle.await??;
    Ok(())
}

// R4: Partial write protection
#[tokio::test]
async fn r4_partial_write_protection() -> Result<()> {
    let relay: RelayInstance = RelayInstance::new(RelayOptions::default()).await?;
    let client = relay.client();

    // Publish v1
    client.publish(&default_runtime_config()).await?;

    // Make LKG directory read-only
    let lkg_dir = relay.lkg_path.parent().unwrap();
    let mut perms = fs::metadata(lkg_dir)?.permissions();
    perms.set_mode(0o555);
    fs::set_permissions(lkg_dir, perms)?;

    let mut config_v2 = default_runtime_config();
    config_v2.server.listen_addr = "127.0.0.1:9092".parse().unwrap();
    let _ = client.publish(&config_v2).await;

    // Restore permissions
    let mut perms = fs::metadata(lkg_dir)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(lkg_dir, perms)?;

    // Restart Relay
    let relay_v2: RelayInstance = relay.restart().await?;
    let status = relay_v2.client().status().await?;

    // Expect v1 because v2 LKG write failed
    assert_eq!(status.version, 1);

    Ok(())
}

// R5: Observability
#[tokio::test]
async fn r5_observability() -> Result<()> {
    let relay: RelayInstance = RelayInstance::new(RelayOptions::default()).await?;
    let client = relay.client();

    let metrics_before = client.metrics().await?;

    // Perform actions that should affect metrics
    let _ = client.publish(&default_runtime_config()).await; // OK
    let _ = client.publish_raw(b"invalid pvs".to_vec()).await; // Fail
    let current_version = client.status().await?.version;
    let _ = client.long_poll(current_version, 10).await; // Long poll

    let metrics_after = client.metrics().await?;

    let ok_before =
        get_metric_value(&metrics_before, "pavis_relay_publish_ok_total").unwrap_or(0.0);
    let ok_after = get_metric_value(&metrics_after, "pavis_relay_publish_ok_total").unwrap_or(0.0);
    assert_eq!(
        ok_after,
        ok_before + 1.0,
        "publish ok metric did not increment"
    );

    let fail_before =
        get_metric_value(&metrics_before, "pavis_relay_publish_fail_total").unwrap_or(0.0);
    let fail_after =
        get_metric_value(&metrics_after, "pavis_relay_publish_fail_total").unwrap_or(0.0);
    assert_eq!(
        fail_after,
        fail_before + 1.0,
        "publish fail metric did not increment"
    );

    let long_poll_before =
        get_metric_value(&metrics_before, "pavis_relay_longpoll_wait_total").unwrap_or(0.0);
    let long_poll_after =
        get_metric_value(&metrics_after, "pavis_relay_longpoll_wait_total").unwrap_or(0.0);
    assert!(
        long_poll_after > long_poll_before,
        "long poll metric did not increment"
    );

    Ok(())
}

fn get_metric_value(metrics: &str, name: &str) -> Option<f64> {
    metrics
        .lines()
        .find(|line| line.starts_with(name))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|value| value.parse::<f64>().ok())
}

// R6: Ingest Debouncing
#[tokio::test]
async fn r6_ingest_debouncing() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;
    options.ingest_debounce_ms = 200;

    let relay: RelayInstance = RelayInstance::new(options).await?;
    let client = relay.client();

    sleep(Duration::from_millis(500)).await;
    let status = client.status().await?;
    let initial_version = status.version;

    let config_path = relay.ingest_path.as_ref().unwrap();
    for i in 0..5 {
        let content = format!("server:\n  listen_addr: \"127.0.0.1:808{i}\"\n");
        fs::write(config_path, content)?;
        sleep(Duration::from_millis(20)).await;
    }

    sleep(Duration::from_millis(1000)).await;

    let status = client.status().await?;
    assert_eq!(status.version, initial_version + 1);

    Ok(())
}

// R7: Persistence Recovery
#[tokio::test]
async fn r7_persistence_recovery() -> Result<()> {
    let relay: RelayInstance = RelayInstance::new(RelayOptions::default()).await?;
    let client = relay.client();

    client.publish(&default_runtime_config()).await?;

    let relay_restarted: RelayInstance = relay.restart().await?;
    let status = relay_restarted.client().status().await?;

    assert_eq!(status.version, 1);

    Ok(())
}

// R8: Codec Validation
#[tokio::test]
async fn r8_codec_validation() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;

    let relay: RelayInstance = RelayInstance::new(options).await?;
    let client = relay.client();
    let config_path = relay.ingest_path.as_ref().unwrap();

    sleep(Duration::from_millis(500)).await;
    let v_start = client.status().await?.version;

    // Write invalid YAML
    fs::write(config_path, "server: { invalid_syntax: [")?;
    sleep(Duration::from_millis(500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, v_start);

    // Write valid YAML
    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8080\"")?;
    sleep(Duration::from_millis(500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, v_start + 1);

    Ok(())
}

// R9: File Replacement
#[tokio::test]
async fn r9_file_replacement() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;

    let relay: RelayInstance = RelayInstance::new(options).await?;
    let client = relay.client();
    let config_path = relay.ingest_path.as_ref().unwrap();

    sleep(Duration::from_millis(500)).await;
    let v_start = client.status().await?.version;

    let tmp_path = config_path.with_extension("tmp");
    fs::write(&tmp_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;
    fs::rename(&tmp_path, config_path)?;

    sleep(Duration::from_millis(500)).await;
    let status = client.status().await?;
    assert_eq!(status.version, v_start + 1);

    Ok(())
}

// R10: Startup with Corrupted LKG
#[tokio::test]
async fn r10_startup_corrupted_lkg() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let lkg_path = temp_dir.path().join("config.pvs");
    fs::write(&lkg_path, b"GARBAGE")?;

    let mut options = RelayOptions::default();
    options.lkg_path = Some(lkg_path);

    let result: Result<RelayInstance> = RelayInstance::new(options).await;
    assert!(result.is_err());

    Ok(())
}

// R11: Rapid Toggle
#[tokio::test]
async fn r11_rapid_toggle() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;
    options.ingest_debounce_ms = 100;

    let relay: RelayInstance = RelayInstance::new(options).await?;
    let client = relay.client();
    let config_path = relay.ingest_path.as_ref().unwrap();

    sleep(Duration::from_millis(500)).await;
    let v_start = client.status().await?.version;

    // Valid
    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;
    sleep(Duration::from_millis(200)).await;

    // Invalid
    fs::write(config_path, "server: [")?;
    sleep(Duration::from_millis(200)).await;

    // Valid
    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8082\"")?;
    sleep(Duration::from_millis(200)).await;

    let status = client.status().await?;
    // v_start -> v_valid1 (inc) -> v_invalid (no inc) -> v_valid2 (inc)
    assert_eq!(status.version, v_start + 2);

    Ok(())
}

// R13: Transient Permission Failure
#[tokio::test]
async fn r13_transient_permission_failure() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;

    let relay: RelayInstance = RelayInstance::new(options).await?;
    let client = relay.client();
    let config_path = relay.ingest_path.as_ref().unwrap();

    sleep(Duration::from_millis(500)).await;
    let v_start = client.status().await?.version;

    let mut perms = fs::metadata(config_path)?.permissions();
    perms.set_mode(0o000);
    fs::set_permissions(config_path, perms)?;

    sleep(Duration::from_millis(500)).await;

    let mut perms = fs::metadata(config_path)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(config_path, perms)?;

    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;

    sleep(Duration::from_millis(500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, v_start + 1);

    Ok(())
}

// R14: Transient Empty File
#[tokio::test]
async fn r14_transient_empty_file() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;

    let relay: RelayInstance = RelayInstance::new(options).await?;
    let client = relay.client();
    let config_path = relay.ingest_path.as_ref().unwrap();

    sleep(Duration::from_millis(500)).await;
    let v_start = client.status().await?.version;

    fs::write(config_path, "")?;
    sleep(Duration::from_millis(500)).await;

    let status = client.status().await?;
    // Assuming empty file doesn't increment or fails.
    assert_eq!(status.version, v_start);

    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;
    sleep(Duration::from_millis(500)).await;

    let status_final = client.status().await?;
    assert!(status_final.version > status.version);

    Ok(())
}

// R15: Artifact Size Limits
#[tokio::test]
async fn r15_artifact_size_limits() -> Result<()> {
    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;
    // Set extremely small limit
    options.max_pvs_bytes = Some(10);

    let relay: RelayInstance = RelayInstance::new(options).await?;
    let client = relay.client();
    let config_path = relay.ingest_path.as_ref().unwrap();

    sleep(Duration::from_millis(500)).await;
    let v_start = client.status().await?.version;

    // Write valid but large config
    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;
    sleep(Duration::from_millis(500)).await;

    // Should NOT update because it exceeds 10 bytes
    let status = client.status().await?;
    assert_eq!(status.version, v_start);

    Ok(())
}
