use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;

#[tokio::test]
async fn r2_reject_invalid_pvs() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();

    let start_version = client.status().await?.version;

    // Publish corrupted PVS bytes
    let result = client.publish_raw(b"not-a-pvs".to_vec()).await;
    assert!(result.is_err());

    // Verify version didn't increment
    let status = client.status().await?;
    assert_eq!(status.version, start_version);

    Ok(())
}
