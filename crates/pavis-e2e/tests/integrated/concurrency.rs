use anyhow::Result;

use super::support::{
    PavisEnv, pavis_target, publish, relay_env, runtime_config, upstreams, wait_for_version,
};

#[tokio::test]
async fn integrated_multiple_runtimes_converge() -> Result<()> {
    if std::env::var("TEST_MODE").unwrap_or_else(|_| "binary".to_string()) == "docker" {
        eprintln!("skipping concurrency test in docker mode");
        return Ok(());
    }
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };

    let seed_target = pavis_target()?;
    let config_v1 = runtime_config(
        seed_target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    let response = publish(relay.base_url(), 1, &config_v1).await?;
    assert!(response.status().is_success());

    let mut pavis_envs = Vec::new();
    for _ in 0..3 {
        let target = pavis_target()?;
        let config = runtime_config(
            target.listen_addr,
            ("upstream-a", upstreams.a),
            ("upstream-b", upstreams.b),
            "upstream-a",
        );
        let env = PavisEnv::new(&config, target.host_port, relay.base_url())?;
        pavis_envs.push(env);
    }

    let config_v2 = runtime_config(
        seed_target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-b",
    );
    let config_v3 = runtime_config(
        seed_target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );
    let _ = publish(relay.base_url(), 2, &config_v2).await?;
    let _ = publish(relay.base_url(), 3, &config_v3).await?;

    for env in &pavis_envs {
        wait_for_version(&env.version_path(), 3).await?;
    }

    Ok(())
}
