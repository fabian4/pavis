use anyhow::Result;
use pavis_core::{AccessLogConfig, RuntimeConfig, ServerConfig, TelemetryConfig};
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::time::Duration;
use tokio::time::sleep;

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
async fn r3_long_poll_semantics() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false).await?;
    let client = scenario.relay.client();
    let v_start = client.status().await?.version;

    // Publish v1
    scenario.apply_config(&default_runtime_config()).await?;
    scenario.wait_for_relay_version(v_start + 1).await?;

    // Wait for timeout (304)
    let start = std::time::Instant::now();
    let poll_res: Option<(u64, Vec<u8>)> = client.long_poll(v_start + 1, 1000).await?;
    assert!(poll_res.is_none()); // 304
    assert!(start.elapsed() >= Duration::from_millis(1000));

    // Wait for update via background apply
    let config_path = scenario.relay.ingest_path.clone().unwrap();
    let handle = tokio::spawn(async move {
        sleep(Duration::from_millis(500)).await;
        let mut cfg = default_runtime_config();
        cfg.server.listen_addr = "127.0.0.1:9091".parse().unwrap();
        let yaml = pavis_e2e::support::to_yaml(&cfg);
        fs::write(config_path, yaml)
    });

    let poll_res: Option<(u64, Vec<u8>)> = client.long_poll(v_start + 1, 4000).await?;
    assert!(poll_res.is_some());
    let (version, _) = poll_res.unwrap();
    assert_eq!(version, v_start + 2);

    handle.await??;
    Ok(())
}

use std::fs;
