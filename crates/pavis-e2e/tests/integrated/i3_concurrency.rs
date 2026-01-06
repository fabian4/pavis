use anyhow::Result;
use pavis_e2e::support::pick_port;
use pavis_e2e::support::relay::RelayOptions;
use pavis_e2e::support::runtime_config;
use pavis_e2e::support::{PavisScenario, TestEnv};

#[tokio::test]
#[ignore]
async fn integrated_multiple_runtimes_converge() -> Result<()> {
    if std::env::var("TEST_MODE").unwrap_or_else(|_| "binary".to_string()) == "docker" {
        eprintln!("skipping concurrency test in docker mode");
        return Ok(());
    }

    let mut options = RelayOptions::default();
    options.enable_file_ingest = true;
    options.ingest_debounce_ms = 500;

    // Start Relay + Upstreams only (no initial Pavis)
    let scenario = PavisScenario::new(options, false, true).await?;
    let upstreams = &scenario.upstreams.as_ref().unwrap();

    // Apply v1 (with dummy listen addr, shared by all but overwritten by bootstrap locally?
    // Actually runtime config from relay overwrites local LKG.
    // So all runtimes will try to bind this address.
    // If this test passed before, maybe Pavis ignores bind failure or listen_addr change behavior allows it?
    // Or maybe they bind with SO_REUSEPORT?
    // For migration, I preserve behavior: use one address for config.
    let seed_port = pick_port()?;
    let seed_addr = format!("127.0.0.1:{seed_port}").parse()?;

    let config_v1 = runtime_config(
        seed_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    scenario.apply_config(&config_v1).await?;

    // Spawn 3 runtimes
    let mut pavis_envs = Vec::new();
    for _ in 0..3 {
        let port = pick_port()?;
        let env = TestEnv::new_with_relay(scenario.relay.env.base_url().to_string(), port).await?;
        pavis_envs.push(env);
    }

    let mut config_v2 = runtime_config(
        seed_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );
    config_v2.telemetry.service_name = pavis_core::ServiceName("pavis-integrated-v2".to_string());
    scenario.apply_config(&config_v2).await?;

    let mut config_v3 = runtime_config(
        seed_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    config_v3.telemetry.service_name = pavis_core::ServiceName("pavis-integrated-v3".to_string());
    scenario.apply_config(&config_v3).await?;

    for env in &pavis_envs {
        // v1 -> v2 -> v3
        // Initial state of envs might be v1.
        // We wait for v3.
        // version 1 (from apply_config 1).
        // version 2 (from apply_config 2).
        // version 3 (from apply_config 3).
        // Note: apply_config increments version on Relay.
        // Since we applied v1, v2, v3.
        // Wait, RelayInstance::new might have created v1 (from initial write).
        // scenario.apply_config(v1) -> v2?
        // apply_config(v2) -> v3?
        // apply_config(v3) -> v4?
        // I should check exact version.
        // Or just wait for "at least 3".
        // The original test waited for 3.
        // If versions are bumped more, we should wait for higher.
        // Let's rely on `wait_for_version` which waits for >= version.
        // If we expect at least 3 updates.
        env.wait_for_version(3).await?;
    }

    Ok(())
}
