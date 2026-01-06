use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
#[cfg(unix)]
async fn r13_transient_permission_failure() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
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

    fs::write(
        config_path,
        "listeners:\n  - name: \"default\"\n    address: \"127.0.0.1:8081\"",
    )?;
    scenario.wait_for_relay_version(v_start + 1).await?;

    let status = client.status().await?;
    assert_eq!(status.version, v_start + 1);

    Ok(())
}
