use anyhow::Result;
use pavis_e2e::support::find_project_root;
use pavis_e2e::support::generate_pvs;
use pavis_e2e::support::resolve_docker_service_ip;
use pavis_e2e::support::tls_support_config;
use pavis_e2e::support::write_config;
use reqwest::Client;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

fn generate_self_signed_cert(cert_path: &PathBuf, key_path: &PathBuf) {
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            key_path.to_str().expect("key path"),
            "-out",
            cert_path.to_str().expect("cert path"),
            "-subj",
            "/CN=localhost",
            "-days",
            "1",
        ])
        .status()
        .expect("failed to run openssl");
    assert!(status.success(), "openssl failed to generate certs");
}

#[tokio::test]
async fn test_tls_support() {
    let mode = std::env::var("TEST_MODE").unwrap_or_else(|_| "binary".to_string());
    let project_root = find_project_root().expect("find project root");

    // 1. Setup Paths & Certs
    let (cert_host_path, key_host_path, cert_config_path, key_config_path, upstream_host) =
        if mode == "docker" {
            let certs_dir = project_root.join("crates/pavis-e2e/config/certs");
            fs::create_dir_all(&certs_dir).unwrap();
            (
                certs_dir.join("tls_support.pem"),
                certs_dir.join("tls_support.key"),
                "/etc/pavis/certs/tls_support.pem".to_string(),
                "/etc/pavis/certs/tls_support.key".to_string(),
                "backend-v1",
            )
        } else {
            let tmp_dir = std::env::temp_dir().join("pavis_test_tls");
            let _ = fs::remove_dir_all(&tmp_dir);
            fs::create_dir_all(&tmp_dir).unwrap();
            let cert = tmp_dir.join("cert.pem");
            let key = tmp_dir.join("key.pem");
            (
                cert.clone(),
                key.clone(),
                cert.to_string_lossy().to_string(),
                key.to_string_lossy().to_string(),
                "127.0.0.1",
            )
        };

    generate_self_signed_cert(&cert_host_path, &key_host_path);

    // 2. Generate Config
    let upstream_host = if mode == "docker" {
        resolve_docker_service_ip(&project_root, upstream_host).expect("resolve backend-v1 IP")
    } else {
        upstream_host.to_string()
    };
    let config_path = if mode == "docker" {
        project_root.join("crates/pavis-e2e/config/generated_tls_support.yaml")
    } else {
        std::env::temp_dir().join("pavis_test_tls/config.yaml")
    };
    let config = tls_support_config(
        "0.0.0.0:8443",
        &cert_config_path,
        &key_config_path,
        &upstream_host,
        8081,
    );
    write_config(&config_path, &config).expect("write tls config");

    // 3. Start/Restart Pavis
    let mut process = None;

    let release_dir = project_root.join("target/release");
    let debug_dir = project_root.join("target/debug");

    let find_binary = |name: &str| -> Result<PathBuf> {
        let release_bin = release_dir.join(name);
        if release_bin.exists() {
            return Ok(release_bin);
        }
        let debug_bin = debug_dir.join(name);
        if debug_bin.exists() {
            return Ok(debug_bin);
        }
        Err(anyhow::anyhow!(
            "Binary '{}' not found. Run cargo build.",
            name
        ))
    };

    if mode == "docker" {
        println!("🐳 Restarting Pavis Container...");
        let pavctl_bin = find_binary("pavctl").expect("pavctl binary not found");
        let shared_config = project_root.join("crates/pavis-e2e/config/generated_config.pvs");

        generate_pvs(&pavctl_bin, &config_path, &shared_config).expect("generate config");

        let compose_file = project_root.join("crates/pavis-e2e/config/docker-compose-pavis.yaml");
        let status = Command::new("docker")
            .args([
                "compose",
                "-f",
                compose_file.to_str().unwrap(),
                "up",
                "-d",
                "--force-recreate",
                "pavis",
            ])
            .status()
            .expect("Failed to run docker compose");
        assert!(status.success());
        sleep(Duration::from_secs(5)).await; // Wait for container start
    } else {
        let pavis_bin = find_binary("pavis").expect("Pavis binary not found");

        // Generate YAML to PVS
        let pavctl_bin = find_binary("pavctl").expect("pavctl binary not found");
        let output_pvs = config_path.with_extension("pvs");

        generate_pvs(&pavctl_bin, &config_path, &output_pvs).expect("generate config");

        println!("🚀 Starting Pavis Binary ({:?})...", output_pvs);
        let child = Command::new(pavis_bin)
            .arg("--config")
            .arg(&output_pvs)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to start pavis");
        process = Some(child);
        sleep(Duration::from_secs(2)).await;
    }

    // 4. Make Request
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let resp = client.get("https://localhost:8443/").send().await;

    // Assert & Consume before cleanup
    match resp {
        Ok(r) => {
            assert!(r.status().is_success(), "Response: {:?}", r.status());
            let text = r.text().await.unwrap();
            println!("Response: {}", text);
            assert!(
                text.contains("backend-v1") || text.contains("echo-server"),
                "Response should be from echo-server"
            );
        }
        Err(e) => {
            panic!("Request failed: {}", e);
        }
    }

    // 5. Cleanup
    if let Some(mut child) = process {
        let _ = child.kill();
    }
    if mode == "binary" {
        let _ = fs::remove_dir_all(config_path.parent().unwrap());
    }
}
