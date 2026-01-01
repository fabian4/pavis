use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn r6_ingest_debouncing() -> Result<()> {
    let mut options = RelayOptions::default();
    options.ingest_debounce_ms = 200;

    let scenario = PavisScenario::new(options, false, false).await?;
    let client = scenario.relay.client();

    let initial_version = client.status().await?.version;
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    // Multiple rapid writes
    for i in 0..5 {
        let content = format!("server:\n  listen_addr: \"127.0.0.1:808{i}\"\n");
        fs::write(config_path, content)?;
        sleep(Duration::from_millis(50)).await;
    }

    // Wait for debounce and processing
    sleep(Duration::from_millis(2500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, initial_version + 1);

    Ok(())
}

#[tokio::test]
async fn r11_rapid_toggle() -> Result<()> {
    let mut options = RelayOptions::default();
    options.ingest_debounce_ms = 100;

    let scenario = PavisScenario::new(options, false, false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    // Valid
    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8081\"")?;
    sleep(Duration::from_millis(1000)).await;

    // Invalid
    fs::write(config_path, "server: [")?;
    sleep(Duration::from_millis(1000)).await;

    // Valid
    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8082\"")?;
    sleep(Duration::from_millis(1000)).await;

    let status = client.status().await?;
    // v_start -> v_valid1 (inc) -> v_invalid (no inc) -> v_valid2 (inc)
    assert_eq!(status.version, v_start + 2);

    Ok(())
}
