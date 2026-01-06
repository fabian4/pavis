use anyhow::Result;
use std::time::Duration;

use super::support::{
    PavisEnv, TcpProxy, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body, wait_for_version,
};

#[tokio::test]
async fn integrated_network_partition_recovery() -> Result<()> {
    if std::env::var("TEST_MODE").unwrap_or_else(|_| "binary".to_string()) == "docker" {
        eprintln!(
            "skipping partition test in docker mode (TcpProxy not supported for docker containers yet)"
        );
        return Ok(());
    }

    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };

    // Setup TCP proxy for Relay
    let relay_addr = relay
        .base_url()
        .trim_start_matches("http://")
        .parse::<std::net::SocketAddr>()?;
    let proxy = TcpProxy::new(relay_addr).await?;
    let proxy_url = format!("http://{}", proxy.listen_addr());

    let target = pavis_target()?;

    // 1. Initial State (v1)
    let config_v1 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    publish(relay.base_url(), 1, &config_v1).await?;

    let pavis = PavisEnv::new(&config_v1, target.host_port, &proxy_url)?;
    wait_for_body(pavis.base_url(), &expected_body("A")).await?;

    // 2. Partition
    proxy.set_partition(true);

    // 3. Update Relay to v2 (Pavis won't see it yet)
    let config_v2 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );
    publish(relay.base_url(), 2, &config_v2).await?;

    // Ensure it still serves A (or at least doesn't serve B yet)
    tokio::time::sleep(Duration::from_secs(1)).await;
    let expected_a = expected_body("A");
    wait_for_body(pavis.base_url(), &expected_a).await?;

    // 4. Restore Network
    proxy.set_partition(false);

    // 5. Expect Runtime to converge to v2
    wait_for_body(pavis.base_url(), &expected_body("B")).await?;
    wait_for_version(&pavis.version_path(), 2).await?;

    Ok(())
}
