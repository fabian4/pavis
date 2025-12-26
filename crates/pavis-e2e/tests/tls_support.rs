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
        let binary = project_root.join("target/release/pavis");
        if !binary.exists() {
            // Fallback to debug if release not found (e.g. running cargo test directly)
            let debug_binary = project_root.join("target/debug/pavis");
            if debug_binary.exists() {
                println!("Using debug binary");
            } else {
                panic!("Pavis binary not found. Run cargo build --release first.");
            }
        }

        // We need to find the binary again properly
        let binary = if project_root.join("target/release/pavis").exists() {
            project_root.join("target/release/pavis")
        } else {
            project_root.join("target/debug/pavis")
        };

        println!("🚀 Starting Pavis Binary...");
        let child = Command::new(binary)
            .arg("--config")
            .arg(&config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to start pavis");
        process = Some(child);
        sleep(Duration::from_secs(2)).await;
    }

    // 4. Make Request
    // Pavis listens on 8443 in the config template.
    // In Docker, we map 8080:8080. We need to map 8443 too?
    // The docker-compose only maps 8080.
    // We need to update docker-compose to map 8443 or change config to listen on 8080.
    // The template says: listen_addr: "0.0.0.0:8443"
    // Let's change the template to use a placeholder for port or just use 8443 and map it.
    // Or simpler: Change config to listen on 8080 (if TLS is enabled on 8080).
    // But wait, the template has `listen_addr: "0.0.0.0:8443"`.
    // If I change it to 8080, it conflicts with the default mapping if I don't change compose.
    // Actually, I can just map 8443:8443 in docker-compose.

    // Let's update docker-compose to map 8443 as well.

    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    // If docker, we hit localhost:8443 (mapped). If binary, we hit localhost:8443 (direct).
    // So we need to ensure 8443 is mapped in docker.

    let resp = client.get("https://localhost:8443/").send().await;

    // 5. Cleanup
    if let Some(mut child) = process {
        let _ = child.kill();
    }
    if mode == "docker" {
        // Optional: stop container?
    }
    if mode == "binary" {
        let _ = fs::remove_dir_all(config_path.parent().unwrap());
    }

    // Assert
    match resp {
        Ok(r) => {
            assert!(r.status().is_success(), "Response: {:?}", r.status());
            let text = r.text().await.unwrap();
            // backend-v1 (echo-server) returns JSON usually, or text?
            // echo-server returns JSON by default.
            // But the previous test expected "Hello Backend".
            // The previous test used a custom backend.
            // Now we use echo-server.
            // We should check if it contains "backend-v1" or similar.
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
