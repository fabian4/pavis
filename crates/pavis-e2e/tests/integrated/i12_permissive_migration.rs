use anyhow::Result;
use pavis_core::{ClientAuth, Path as RoutePath, TlsConfig};
use reqwest::Client;
use std::fs;
use std::process::Command;

use super::support::{
    PavisEnv, expected_body, pavis_target, publish, relay_env, runtime_config, upstreams,
};

// TODO: This test requires proper client certificate support in reqwest
// For now, we test the server-side Optional client auth configuration
// Full client cert testing would require native-tls or curl integration
#[tokio::test]
async fn integrated_permissive_migration() -> Result<()> {
    let relay = relay_env().await?;
    let Some(upstreams) = upstreams().await? else {
        return Ok(());
    };
    let target = pavis_target()?;

    let is_docker = std::env::var("TEST_MODE").unwrap_or_default() == "docker";

    // Setup certificate directory
    let tmp_dir = std::env::temp_dir().join("pavis_integrated_permissive");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir)?;

    let server_cert = tmp_dir.join("server_cert.pem");
    let server_key = tmp_dir.join("server_key.pem");
    let ca_cert = tmp_dir.join("ca_cert.pem");
    let ca_key = tmp_dir.join("ca_key.pem");

    // 1. Generate CA certificate
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            ca_key.to_str().unwrap(),
            "-out",
            ca_cert.to_str().unwrap(),
            "-subj",
            "/CN=Test CA",
            "-days",
            "1",
        ])
        .status()?;
    assert!(status.success(), "Failed to generate CA certificate");

    // 2. Generate server certificate signed by CA
    let status = Command::new("openssl")
        .args([
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            server_key.to_str().unwrap(),
            "-out",
            tmp_dir.join("server.csr").to_str().unwrap(),
            "-subj",
            "/CN=localhost",
        ])
        .status()?;
    assert!(status.success(), "Failed to generate server CSR");

    let status = Command::new("openssl")
        .args([
            "x509",
            "-req",
            "-in",
            tmp_dir.join("server.csr").to_str().unwrap(),
            "-CA",
            ca_cert.to_str().unwrap(),
            "-CAkey",
            ca_key.to_str().unwrap(),
            "-CAcreateserial",
            "-out",
            server_cert.to_str().unwrap(),
            "-days",
            "1",
        ])
        .status()?;
    assert!(status.success(), "Failed to sign server certificate");

    // 3. Configure Pavis with Optional client auth
    let mut config = runtime_config(
        target.listen_addr,
        ("upstream-a", upstreams.a),
        ("upstream-b", upstreams.b),
        "upstream-a",
    );

    let (server_cert_path, server_key_path, ca_cert_path) = if is_docker {
        (
            "/pavis/certs/server_cert.pem".to_string(),
            "/pavis/certs/server_key.pem".to_string(),
            "/pavis/certs/ca_cert.pem".to_string(),
        )
    } else {
        (
            server_cert.to_str().unwrap().to_string(),
            server_key.to_str().unwrap().to_string(),
            ca_cert.to_str().unwrap().to_string(),
        )
    };

    config.listeners[0].tls = TlsConfig::Enabled {
        cert_path: RoutePath(server_cert_path),
        key_path: RoutePath(server_key_path),
        client_auth: ClientAuth::Optional {
            ca_path: RoutePath(ca_cert_path),
        },
    };

    // 4. Publish and start Pavis
    publish(relay.base_url(), 1, &config).await?;
    let mut pavis = PavisEnv::new(&config, target.host_port, relay.base_url())?;

    if is_docker {
        let pavis_certs_dir = pavis.work_dir.join("certs");
        fs::create_dir_all(&pavis_certs_dir)?;
        fs::copy(&server_cert, pavis_certs_dir.join("server_cert.pem"))?;
        fs::copy(&server_key, pavis_certs_dir.join("server_key.pem"))?;
        fs::copy(&ca_cert, pavis_certs_dir.join("ca_cert.pem"))?;
        pavis.restart()?;
    }

    let base_url = format!("https://localhost:{}", target.host_port);

    // 5. Test: Client WITHOUT certificate -> Should succeed in Optional mode
    let client_without_cert = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let mut success_without_cert = false;
    for _ in 0..20 {
        if let Ok(resp) = client_without_cert.get(&base_url).send().await {
            if resp.status().is_success() {
                let text = resp.text().await?;
                if text.contains(&expected_body("A")) {
                    success_without_cert = true;
                    break;
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    assert!(
        success_without_cert,
        "Client without certificate should succeed in Optional mode (permissive migration)"
    );

    // TODO: Add test case with client certificate using curl or native-tls
    // This would verify that clients WITH certificates are also accepted
    // and their identity is extracted for use in routing/RBAC

    Ok(())
}
