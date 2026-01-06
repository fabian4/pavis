use anyhow::Result;

use super::support::{
    PavisEnv, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body, wait_for_version,
};

#[tokio::test]
async fn integrated_data_plane_recovery() -> Result<()> {
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    // 1. Initial Publish v1
    let config_v1 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    publish(relay.base_url(), 1, &config_v1).await?;

    let mut pavis = PavisEnv::new(&config_v1, target.host_port, relay.base_url())?;
    wait_for_body(pavis.base_url(), &expected_body("A")).await?;

    // 2. Publish v2 while running
    let config_v2 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );
    publish(relay.base_url(), 2, &config_v2).await?;
    wait_for_body(pavis.base_url(), &expected_body("B")).await?;

    // 3. Kill and Restart Runtime
    pavis.restart()?;

    // 4. Expect it to pick up v2 immediately and restore traffic
    wait_for_body(pavis.base_url(), &expected_body("B")).await?;
    wait_for_version(&pavis.version_path(), 2).await?;

    Ok(())
}
