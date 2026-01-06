use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[cfg(unix)]
async fn r12_symlink_updates() -> Result<()> {
    if std::env::var("TEST_MODE").unwrap_or_default() == "docker" {
        println!("Skipping r12 in docker mode (symlink complex setup)");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let data_dir = temp_dir.path().join("data");
    fs::create_dir(&data_dir)?;

    let v1_path = data_dir.join("v1.yaml");
    let v2_path = data_dir.join("v2.yaml");
    let link_path = temp_dir.path().join("config.yaml");

    fs::write(&v1_path, "listeners: []")?;
    fs::write(
        &v2_path,
        "listeners: [{name: 'v2', address: '127.0.0.1:8080'}]",
    )?;

    std::os::unix::fs::symlink(&v1_path, &link_path)?;

    let mut options = RelayOptions::default();
    options.ingest_path = Some(link_path.clone());

    let scenario = PavisScenario::new(options, false, false).await?;
    let client = scenario.relay.client();
    let v_start = client.status().await?.version;

    // Update symlink atomically (typical K8s ConfigMap behavior)
    let tmp_link = temp_dir.path().join("config.tmp");
    std::os::unix::fs::symlink(&v2_path, &tmp_link)?;
    fs::rename(&tmp_link, &link_path)?;

    // Wait for either notify or polling fallback (2s)
    sleep(Duration::from_millis(3500)).await;

    let status = client.status().await?;
    assert!(
        status.version > v_start,
        "Version should have incremented after symlink update"
    );

    Ok(())
}
