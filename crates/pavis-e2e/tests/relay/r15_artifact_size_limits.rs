use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn r15_artifact_size_limits() -> Result<()> {
    let mut options = RelayOptions::default();
    options.max_pvs_bytes = Some(10);

    let scenario = PavisScenario::new(options, false, false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    // Write valid but large config
    fs::write(
        config_path,
        "listeners:\n  - name: \"default\"\n    address: \"127.0.0.1:8081\"",
    )?;
    sleep(Duration::from_millis(1500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, v_start);

    Ok(())
}
