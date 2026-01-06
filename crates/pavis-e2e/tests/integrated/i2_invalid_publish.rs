use anyhow::Result;
use reqwest::StatusCode;

use super::support::{
    PavisEnv, client, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body,
};

#[tokio::test]
async fn integrated_invalid_publish_keeps_lkg() -> Result<()> {
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

    let response = client()?
        .post(format!("{}/v1/publish", relay.base_url()))
        .header("X-Pavis-Version", "2")
        .body(b"bad".to_vec())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    wait_for_body(pavis.base_url(), &expected_a).await?;
    Ok(())
}
