use anyhow::Result;
use pavis_core::{
    AccessLogPolicy, Listener, ListenerName, LogLevel, Metrics, RuntimeConfig, ServiceName,
    Telemetry, TlsConfig, TracingPolicy, WorkerCount,
};
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

fn simple_config() -> RuntimeConfig {
    RuntimeConfig {
        listeners: vec![Listener {
            name: ListenerName("default".to_string()),
            address: "127.0.0.1:0".parse().unwrap(),
            workers: WorkerCount::Auto,
            tls: TlsConfig::Disabled,
        }],
        telemetry: Telemetry {
            level: LogLevel::Info,
            pingora: LogLevel::Info,
            service_name: ServiceName("partial-write-test".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: TracingPolicy::Disabled,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    }
}

#[tokio::test]
async fn r4_partial_write_protection() -> Result<()> {
    if std::env::var("TEST_MODE").unwrap_or_default() == "docker" {
        println!("Skipping r4 in docker mode (filesystem manipulation not supported)");
        return Ok(());
    }

    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();

    // Initial publish to have a known LKG
    let config1 = simple_config();
    client.publish(&config1).await?;
    scenario.wait_for_relay_version(2).await?;

    // Give persistence some time to flush
    sleep(Duration::from_millis(500)).await;
    let lkg_v2 = fs::read(&scenario.relay.lkg_path)?;

    let lkg_path = &scenario.relay.lkg_path;

    // Create a directory at the lkg_path to block the rename
    fs::remove_file(lkg_path)?;
    fs::create_dir(lkg_path)?;

    let mut config2 = simple_config();
    config2.telemetry.service_name = ServiceName("should-fail-disk".to_string());

    // The API call SHOULD FAIL because handlers.rs:tokio::fs::write(path, &payload).await is sync/blocking
    let result = client.publish(&config2).await;
    assert!(
        result.is_err(),
        "Publish should have failed due to blocked disk write"
    );

    // Wait for any background attempts
    sleep(Duration::from_millis(1000)).await;

    // Verify LKG is still a directory (not updated to a file)
    assert!(
        fs::metadata(lkg_path)?.is_dir(),
        "LKG path should still be a directory (blocked update)"
    );

    fs::remove_dir(lkg_path)?;
    // Restore file for cleanup
    fs::write(lkg_path, lkg_v2)?;

    Ok(())
}
