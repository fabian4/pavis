use anyhow::Result;
use pavis_core::{AccessLogConfig, RuntimeConfig, ServerConfig, TelemetryConfig};
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;

fn default_runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        server: ServerConfig {
            listen_addr: "127.0.0.1:8080".parse().unwrap(),
            worker_threads: None,
            tls: None,
        },
        telemetry: TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: AccessLogConfig::Disabled,
            tracing: None,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    }
}

#[tokio::test]
async fn r5_observability() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false).await?;
    let client = scenario.relay.client();

    let metrics_before = client.metrics().await?;

    // Perform actions that should affect metrics
    scenario.apply_config(&default_runtime_config()).await?;
    scenario.wait_for_relay_version(1).await?;

    let _ = client.publish_raw(b"invalid pvs".to_vec()).await; // Fail
    let current_version = client.status().await?.version;
    let _ = client.long_poll(current_version, 10).await; // Long poll

    let metrics_after = client.metrics().await?;

    let ok_before =
        get_metric_value(&metrics_before, "pavis_relay_publish_ok_total").unwrap_or(0.0);
    let ok_after = get_metric_value(&metrics_after, "pavis_relay_publish_ok_total").unwrap_or(0.0);
    assert!(ok_after >= ok_before + 1.0);

    let fail_before =
        get_metric_value(&metrics_before, "pavis_relay_publish_fail_total").unwrap_or(0.0);
    let fail_after =
        get_metric_value(&metrics_after, "pavis_relay_publish_fail_total").unwrap_or(0.0);
    assert!(fail_after >= fail_before + 1.0);

    let long_poll_before =
        get_metric_value(&metrics_before, "pavis_relay_longpoll_wait_total").unwrap_or(0.0);
    let long_poll_after =
        get_metric_value(&metrics_after, "pavis_relay_longpoll_wait_total").unwrap_or(0.0);
    assert!(long_poll_after > long_poll_before);

    Ok(())
}

fn get_metric_value(metrics: &str, name: &str) -> Option<f64> {
    metrics
        .lines()
        .find(|line| line.starts_with(name))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|value| value.parse::<f64>().ok())
}
