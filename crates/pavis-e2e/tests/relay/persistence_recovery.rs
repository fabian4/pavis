use anyhow::Result;
use pavis_core::{AccessLogConfig, RuntimeConfig, ServerConfig, TelemetryConfig};
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::fs;
use std::os::unix::fs::PermissionsExt;

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
async fn r7_persistence_recovery() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let v_start = scenario.relay.client().status().await?.version;

    scenario.apply_config(&default_runtime_config()).await?;
    scenario.wait_for_relay_version(v_start + 1).await?;

    let relay_restarted = scenario.relay.restart().await?;
    let status = relay_restarted.client().status().await?;

    // Restart will reload LKG, which usually has version 1 or v_start+1 depending on how init_state is called.
    // In e2e, each scenario is fresh, so it should be v_start + 1.
    assert_eq!(status.version, v_start + 1);

    Ok(())
}

#[tokio::test]
async fn r4_partial_write_protection() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let v_start = scenario.relay.client().status().await?.version;

    // Publish v1
    scenario.apply_config(&default_runtime_config()).await?;
    scenario.wait_for_relay_version(v_start + 1).await?;

    // Make LKG directory read-only
    let lkg_dir = scenario.relay.lkg_path.parent().unwrap();
    let mut perms = fs::metadata(lkg_dir)?.permissions();
    perms.set_mode(0o555);
    fs::set_permissions(lkg_dir, perms)?;

    let mut config_v2 = default_runtime_config();
    config_v2.server.listen_addr = "127.0.0.1:9092".parse().unwrap();
    scenario.apply_config(&config_v2).await?;

    // Restore permissions
    let mut perms = fs::metadata(lkg_dir)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(lkg_dir, perms)?;

    // Restart Relay
    let relay_restarted = scenario.relay.restart().await?;
    let status = relay_restarted.client().status().await?;

    // Expect v_start + 1 because v_start + 2 LKG write failed
    assert_eq!(status.version, v_start + 1);

    Ok(())
}
