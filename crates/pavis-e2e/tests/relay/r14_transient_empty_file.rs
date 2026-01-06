use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn r14_transient_empty_file() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    fs::write(config_path, "")?;
    sleep(Duration::from_millis(1500)).await;

    let status = client.status().await?;
    assert!(status.version >= v_start);
    let v_after_empty = status.version;

    fs::write(
        config_path,
        "listeners:\n  - name: \"default\"\n    address: \"127.0.0.1:8081\"",
    )?;
    scenario.wait_for_relay_version(v_after_empty + 1).await?;

    Ok(())
}
