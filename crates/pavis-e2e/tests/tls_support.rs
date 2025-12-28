use anyhow::Result;
use reqwest::Client;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

fn find_project_root() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap();
    loop {
        if dir.join("Cargo.lock").exists() {
            return dir;
        }
        if !dir.pop() {
            panic!("Could not find project root");
        }
    }
}

#[tokio::test]
async fn test_tls_support() {
    let mode = std::env::var("TEST_MODE").unwrap_or_else(|_| "binary".to_string());
    let project_root = find_project_root();

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

    fs::write(&cert_host_path, include_str!("fixtures/cert.pem")).unwrap();
    fs::write(&key_host_path, include_str!("fixtures/key.pem")).unwrap();

    // 2. Generate Config
    let template = include_str!("../config/templates/tls_support.yaml");
    let config_content = template
        .replacen("{}", &cert_config_path, 1)
        .replacen("{}", &key_config_path, 1)
        .replacen("{}", "8081", 1); // Upstream port

    // We need to patch the upstream host in the template or config
    // The template currently has:
    // upstreams:
    //   - name: "backend"
    //     endpoints:
    //       - ip: "127.0.0.1"
    //         port: {}
    // We need to change 127.0.0.1 to upstream_host if docker
    let config_content = config_content.replace("127.0.0.1", upstream_host);

    let config_path = if mode == "docker" {
        project_root.join("crates/pavis-e2e/config/generated_tls_support.yaml")
    } else {
        std::env::temp_dir().join("pavis_test_tls/config.yaml")
    };
    fs::write(&config_path, config_content).unwrap();

    // 3. Start/Restart Pavis
    let mut process = None;

    if mode == "docker" {
        println!("🐳 Restarting Pavis Container...");
        let shared_config = project_root.join("crates/pavis-e2e/config/generated_config.yaml");
        fs::copy(&config_path, &shared_config).unwrap();

        let compose_file = project_root.join("crates/pavis-e2e/config/docker-compose.yaml");
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

        let pavis_bin = find_binary("pavis").expect("Pavis binary not found");

        // Compile YAML to PVS
        let pavis_cli_bin = find_binary("pavis-cli").expect("Pavis CLI binary not found");
        let output_pvs = config_path.with_extension("pvs");

        println!(
            "🔨 Compiling YAML to PVS: {:?} -> {:?}",
            config_path, output_pvs
        );
        let status = Command::new(&pavis_cli_bin)
            .arg("compile")
            .arg("--input")
            .arg(&config_path)
            .arg("--output")
            .arg(&output_pvs)
            .status()
            .expect("Failed to run pavis-cli");

        assert!(status.success(), "Failed to compile config");

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

    // 5. Cleanup
    if let Some(mut child) = process {
        let _ = child.kill();
    }
    if mode == "binary" {
        let _ = fs::remove_dir_all(config_path.parent().unwrap());
    }

    // Assert
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
}
