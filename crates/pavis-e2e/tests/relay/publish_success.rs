use anyhow::Result;
use pavis_core::{AccessLogConfig, RuntimeConfig, ServerConfig, TelemetryConfig};
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use pavis_pvs::PAVIS_MAGIC;
use std::fs;

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
async fn r1_publish_success() -> Result<()> {
    let mut options = RelayOptions::default();
    options.ingest_debounce_ms = 500;
    let scenario = PavisScenario::new(options, false).await?;
    let client = scenario.relay.client();

    // Initial state
    let status = client.status().await?;
    let v_start = status.version;

    // Publish v1 via file ingest
    let config_v1 = default_runtime_config();
    scenario.apply_config(&config_v1).await?;

    // Verify status update
    scenario.wait_for_relay_version(v_start + 1).await?;
    let status = client.status().await?;
    assert_eq!(status.version, v_start + 1);
    assert!(!status.checksum.is_empty());

    // Verify v1 artifact
    let artifact: Vec<u8> = client.get_artifact(v_start + 1).await?;
    assert!(artifact.starts_with(PAVIS_MAGIC));

    // Verify LKG on disk
    let lkg_bytes = fs::read(&scenario.relay.lkg_path)?;
    assert!(lkg_bytes.starts_with(PAVIS_MAGIC));

    // Publish v2
    let mut config_v2 = default_runtime_config();
    config_v2.server.listen_addr = "127.0.0.1:9090".parse().unwrap();
    scenario.apply_config(&config_v2).await?;

    // Verify LKG updated
    scenario.wait_for_relay_version(v_start + 2).await?;
    let status_v2 = client.status().await?;
    assert_eq!(status_v2.version, v_start + 2);
    assert_ne!(status_v2.checksum, status.checksum);

    Ok(())
}
