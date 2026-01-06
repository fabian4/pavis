use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn r9_file_replacement() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    // Simulate atomic file replacement (mv)
    let tmp_path = config_path.with_extension("tmp");
    fs::write(
        &tmp_path,
        "listeners:\n  - name: \"replaced\"\n    address: \"127.0.0.1:8081\"",
    )?;
    fs::rename(&tmp_path, config_path)?;

    // Wait for debounce and processing
    sleep(Duration::from_millis(1500)).await;

    let status = client.status().await?;
    assert!(status.version > v_start);

    Ok(())
}
