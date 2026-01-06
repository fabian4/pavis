use anyhow::Result;
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;

#[tokio::test]
async fn r5_observability() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();

    let status = client.status().await?;
    assert!(!status.checksum.is_empty());

    let metrics_before = client.metrics().await?;

    // Trigger some events
    let _ = client.publish_raw(b"invalid pvs".to_vec()).await; // Fail

    let metrics_after = client.metrics().await?;

    fn get_metric_value(metrics: &str, name: &str) -> Option<f64> {
        metrics
            .lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().last())
            .and_then(|v| v.parse().ok())
    }

    let fail_before =
        get_metric_value(&metrics_before, "pavis_relay_publish_fail_total").unwrap_or(0.0);
    let fail_after =
        get_metric_value(&metrics_after, "pavis_relay_publish_fail_total").unwrap_or(0.0);
    assert!(fail_after > fail_before);

    Ok(())
}
