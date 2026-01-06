use anyhow::Result;

use super::support::{
    PavisEnv, client, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
    wait_for_body,
};

#[tokio::test]
async fn integrated_observability_headers_and_metrics() -> Result<()> {
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
        .get(format!("{}/v1/config", relay.base_url()))
        .header("X-Pavis-Version", "0")
        .send()
        .await?;
    assert!(response.status().is_success());
    assert!(
        response
            .headers()
            .get("x-pavis-version")
            .and_then(|value| value.to_str().ok())
            .is_some()
    );
    assert!(
        response
            .headers()
            .get("x-pavis-checksum")
            .and_then(|value| value.to_str().ok())
            .is_some()
    );

    let metrics = client()?
        .get(format!("{}/v1/metrics", relay.base_url()))
        .send()
        .await?
        .text()
        .await?;
    assert!(metrics.contains("pavis_relay_publish_ok_total"));
    assert!(metrics.contains("pavis_relay_longpoll_wait_total"));

    Ok(())
}
