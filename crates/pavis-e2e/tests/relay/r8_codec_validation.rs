use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn r8_codec_validation() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    // Write invalid YAML
    fs::write(config_path, "invalid: [unclosed bracket")?;

    // Wait to ensure no update happens
    sleep(Duration::from_millis(1500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, v_start);

    Ok(())
}
