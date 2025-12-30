use anyhow::Result;

use super::support::{
    PavisEnv, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body,
};

#[tokio::test]
async fn integrated_publish_and_apply_updates() -> Result<()> {
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    let config_v1 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    let response = publish(relay.base_url(), 1, &config_v1).await?;
    assert!(response.status().is_success());

    let pavis = PavisEnv::new(&config_v1, target.host_port, relay.base_url())?;
    let expected_a = expected_body("A");
    wait_for_body(pavis.base_url(), &expected_a).await?;

    let config_v2 = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );
    let response = publish(relay.base_url(), 2, &config_v2).await?;
    assert!(response.status().is_success());
    let expected_b = expected_body("B");
    wait_for_body(pavis.base_url(), &expected_b).await?;

    Ok(())
}
