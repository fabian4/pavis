use anyhow::Result;
use pavis_core::{
    AccessLogPolicy, Listener, ListenerName, LogLevel, Metrics, RuntimeConfig, ServiceName,
    Telemetry, TlsConfig, TracingPolicy, WorkerCount,
};
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
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
            service_name: ServiceName("persistence-test".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: TracingPolicy::Disabled,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    }
}

#[tokio::test]
async fn r7_persistence_recovery() -> Result<()> {
    if std::env::var("TEST_MODE").unwrap_or_default() == "docker" {
        println!("Skipping r7 in docker mode (persistent storage test needs local control)");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let lkg_path = temp_dir.path().join("config.pvs");
    let storage_dir = temp_dir.path().join("artifacts");
    std::fs::create_dir(&storage_dir)?;

    // Ensure clean state: remove any existing LKG file
    if lkg_path.exists() {
        std::fs::remove_file(&lkg_path)?;
    }

    let mut options = RelayOptions::default();
    options.lkg_path = Some(lkg_path.clone());
    options.storage_root = Some(storage_dir.clone());

    // 1. Start Relay and publish config
    {
        let scenario = PavisScenario::new(options.clone(), false, false).await?;
        let client = scenario.relay.client();
        let config = simple_config();
        client.publish(&config).await?;

        // Wait for relay version
        scenario.wait_for_relay_version(2).await?;
        // Ensure persistence flushed by checking file on disk
        let mut flushed = false;
        for _ in 0..20 {
            if let Ok(bytes) = std::fs::read(&lkg_path) {
                if !bytes.is_empty() {
                    flushed = true;
                    break;
                }
            }
            sleep(Duration::from_millis(100)).await;
        }
        assert!(flushed, "LKG file should have been flushed to disk");

        // Explicitly drop to ensure clean shutdown and persistence
        drop(scenario);

        // Give the persistence task time to complete shutdown flush
        sleep(Duration::from_millis(200)).await;
    } // Relay stops here

    // 2. Restart Relay with same storage.
    // The relay should restart at version 1 because it reads the LKG file
    // but always starts fresh at version 1 (LKG is just for recovery, not version tracking).
    {
        let scenario = PavisScenario::new(options, false, false).await?;
        let client = scenario.relay.client();
        let status = client.status().await?;
        assert_eq!(
            status.version, 1,
            "Relay should restart at version 1 when LKG exists"
        );
    }

    Ok(())
}
