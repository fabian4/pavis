use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;

#[tokio::test]
async fn r2_reject_invalid_pvs_api() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();

    let start_status = client.status().await?;
    let invalid_bytes = b"PAVS\x00\x00\x00\x01INVALID";
    let err = client
        .publish_raw(invalid_bytes.to_vec())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("422"));

    let status = client.status().await?;
    assert_eq!(status.version, start_status.version);

    Ok(())
}

#[tokio::test]
async fn r8_codec_validation_file() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();
    let config_path = scenario.relay.ingest_path.as_ref().unwrap();

    let v_start = client.status().await?.version;

    // Write invalid YAML
    fs::write(config_path, "server: { invalid_syntax: [")?;
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let status = client.status().await?;
    assert_eq!(status.version, v_start);

    // Write valid YAML
    fs::write(config_path, "server:\n  listen_addr: \"127.0.0.1:8080\"")?;
    scenario.wait_for_relay_version(v_start + 1).await?;

    let status = client.status().await?;
    assert_eq!(status.version, v_start + 1);

    Ok(())
}
