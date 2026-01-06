use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn r11_rapid_toggle() -> Result<()> {
    let mut options = RelayOptions::default();
    options.ingest_debounce_ms = 100;
    let scenario = PavisScenario::new(options, false, false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    // The relay might start at v1 if it ingested the initial default config.
    let v_start = client.status().await?.version;

    // 1. Valid write
    fs::write(config_path, "listeners: []")?;
    // Wait for debounce (100ms) + processing
    for _ in 0..20 {
        if client.status().await?.version > v_start {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    let v_mid = client.status().await?.version;
    assert!(
        v_mid > v_start,
        "Version should have incremented after first valid write"
    );

    // 2. Invalid write
    fs::write(config_path, "listeners: [")?;
    sleep(Duration::from_millis(1500)).await;

    let v_after_invalid = client.status().await?.version;
    assert_eq!(
        v_after_invalid, v_mid,
        "Version should NOT increment after invalid write"
    );

    // 3. Valid write
    fs::write(
        config_path,
        "listeners:\n  - name: \"final\"\n    address: \"127.0.0.1:8080\"",
    )?;
    for _ in 0..20 {
        if client.status().await?.version > v_after_invalid {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    let v_final = client.status().await?.version;
    assert!(
        v_final > v_after_invalid,
        "Version should increment after second valid write"
    );

    Ok(())
}
