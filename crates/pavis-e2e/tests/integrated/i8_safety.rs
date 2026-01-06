use anyhow::Result;
use std::time::Duration;

use super::support::{
    PavisEnv, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body,
};

#[tokio::test]
async fn integrated_stale_control_plane_rejection() -> Result<()> {
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    // 1. Setup Runtime with high version (v10)
    let config_v10 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    // PavisEnv::new_with_version(config, port, relay_url, version)
    let pavis = PavisEnv::new_with_version(&config_v10, target.host_port, relay.base_url(), 10)?;

    let expected_a = expected_body("A");
    if let Err(e) = wait_for_body(pavis.base_url(), &expected_a).await {
        pavis.print_logs();
        return Err(e);
    }

    // 2. Fresh Relay has nothing or low version. Let's publish v1 to it.
    let config_v1 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );
    publish(relay.base_url(), 1, &config_v1).await?;

    // 3. Expect Runtime to REJECT v1 and stay on v10 (Upstream A)
    // We wait a bit to ensure it had time to poll and reject.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // It should still serve A
    wait_for_body(pavis.base_url(), &expected_a).await?;

    // And if we publish v11, it should update
    let config_v11 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );
    publish(relay.base_url(), 11, &config_v11).await?;

    let expected_b = expected_body("B");
    wait_for_body(pavis.base_url(), &expected_b).await?;

    Ok(())
}
