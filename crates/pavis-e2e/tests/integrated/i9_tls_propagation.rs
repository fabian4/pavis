use anyhow::Result;
use pavis_core::{Path as RoutePath, TlsConfig};
use reqwest::Client;
use std::fs;
use std::process::Command;

use super::support::{
    PavisEnv, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
};

#[tokio::test]
async fn integrated_tls_propagation() -> Result<()> {
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    // 1. Setup Paths & Certs in a shared location
    // We'll use a sub-directory in the relay's work_dir or similar,
    // but Pavis needs to see them.
    // The easiest is to use a fixed path if we are in Docker, or temp dir if in binary.

    let is_docker = std::env::var("TEST_MODE").unwrap_or_default() == "docker";

    let tmp_dir = std::env::temp_dir().join("pavis_integrated_tls");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).unwrap();

    let cert_host_path = tmp_dir.join("cert.pem");
    let key_host_path = tmp_dir.join("key.pem");

    // Generate certs
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            key_host_path.to_str().unwrap(),
            "-out",
            cert_host_path.to_str().unwrap(),
            "-subj",
            "/CN=localhost",
            "-days",
            "1",
        ])
        .status()?;
    assert!(status.success());

    // 2. Prepare Config
    let mut config = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );

    // Update listener to use TLS
    // If docker, we need to map these files into the container.
    // PavisEnv already maps its own work_dir to /pavis.
    // So we should put certs there.

    let (cert_pavis_path, key_pavis_path) = if is_docker {
        (
            "/pavis/certs/cert.pem".to_string(),
            "/pavis/certs/key.pem".to_string(),
        )
    } else {
        (
            cert_host_path.to_str().unwrap().to_string(),
            key_host_path.to_str().unwrap().to_string(),
        )
    };

    config.listeners[0].tls = TlsConfig::Enabled {
        cert_path: RoutePath(cert_pavis_path),
        key_path: RoutePath(key_pavis_path),
        client_auth: pavis_core::ClientAuth::Disabled,
    };

    // 3. Publish to Relay
    publish(relay.base_url(), 1, &config).await?;

    // 4. Start Pavis
    // For docker mode, we must ensure certs are in PavisEnv's work_dir before start
    let mut pavis = PavisEnv::new(&config, target.host_port, relay.base_url())?;

    if is_docker {
        let pavis_certs_dir = pavis.work_dir.join("certs");
        fs::create_dir_all(&pavis_certs_dir)?;
        fs::copy(&cert_host_path, pavis_certs_dir.join("cert.pem"))?;
        fs::copy(&key_host_path, pavis_certs_dir.join("key.pem"))?;
        // We might need to restart pavis if it failed to find certs on initial start
        // but PavisEnv::new already started it.
        // Actually, Pavis should retry or we restart it.
        pavis.restart()?;
    }

    // 5. Verify connectivity via HTTPS
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    // Pavis is listening on host_port
    let https_url = format!("https://localhost:{}", target.host_port);

    // Wait for Pavis to be ready (it might take a moment to load TLS)
    let mut success = false;
    for _ in 0..20 {
        if let Ok(resp) = client.get(&https_url).send().await {
            if resp.status().is_success() {
                let text = resp.text().await?;
                if text.contains(&expected_body("A")) {
                    success = true;
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    assert!(
        success,
        "Should be able to connect via HTTPS and receive expected body"
    );

    Ok(())
}
