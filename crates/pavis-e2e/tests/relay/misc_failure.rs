use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn r10_startup_corrupted_lkg() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let lkg_path = temp_dir.path().join("config.pvs");
    fs::write(&lkg_path, b"GARBAGE")?;

    let mut options = RelayOptions::default();
    options.lkg_path = Some(lkg_path);

    // Initial state setup bypasses PavisScenario for corruption test
    let result: Result<pavis_e2e::support::relay::RelayInstance> =
        pavis_e2e::support::relay::RelayInstance::new(options).await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
async fn r13_transient_permission_failure() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    let mut perms = fs::metadata(config_path)?.permissions();
    perms.set_mode(0o000);
    fs::set_permissions(config_path, perms)?;

    sleep(Duration::from_millis(1500)).await;

    let mut perms = fs::metadata(config_path)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(config_path, perms)?;

    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;
    scenario.wait_for_relay_version(v_start + 1).await?;

    let status = client.status().await?;
    assert_eq!(status.version, v_start + 1);

    Ok(())
}

#[tokio::test]
async fn r14_transient_empty_file() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    fs::write(config_path, "")?;
    sleep(Duration::from_millis(1500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, v_start);

    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;
    scenario.wait_for_relay_version(v_start + 1).await?;

    Ok(())
}

#[tokio::test]
async fn r15_artifact_size_limits() -> Result<()> {
    let mut options = RelayOptions::default();
    options.max_pvs_bytes = Some(10);

    let scenario = PavisScenario::new(options, false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    // Write valid but large config
    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;
    sleep(Duration::from_millis(1500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, v_start);

    Ok(())
}
