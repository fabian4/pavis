use pavis_e2e::utils::find_project_root;
use reqwest::Client;
use std::fs;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_upstream_tls() {
    let mode = std::env::var("TEST_MODE").unwrap_or_else(|_| "binary".to_string());
    let project_root = find_project_root().unwrap();

    // 1. Setup Paths & Certs
    let (
        cert_host_path,
        key_host_path,
        _cert_config_path,
        _key_config_path,
        upstream_host,
        upstream_port,
    ) = if mode == "docker" {
        let certs_dir = project_root.join("crates/pavis-e2e/config/certs");
        fs::create_dir_all(&certs_dir).unwrap();
        (
            certs_dir.join("upstream_tls.pem"),
            certs_dir.join("upstream_tls.key"),
            "/etc/pavis/certs/upstream_tls.pem".to_string(), // Not used in config for upstream TLS usually, but maybe for client certs?
            // Wait, Upstream TLS config in Pavis currently only supports `verify_hostname` and `verify_cert`.
            // It does NOT support client certificates yet.
            // So we don't need to put cert paths in Pavis config for UPSTREAM TLS.
            // We only need to configure Pavis to trust the CA or disable verification.
            // The test disables verification.
            // So we just need to generate certs for the BACKEND to use.
            "/unused".to_string(),
            "backend-tls",
            8443,
        )
    } else {
        let tmp_dir = std::env::temp_dir().join("pavis_test_upstream_tls");
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir).unwrap();
        let cert = tmp_dir.join("cert.pem");
        let key = tmp_dir.join("key.pem");

        // For binary mode, we start a local server on a random port.
        // We'll determine the port later when starting the backend.
        // But we need to return something here.
        // Let's restructure slightly.
        (
            cert,
            key,
            "".to_string(),
            "".to_string(),
            "127.0.0.1",
            0, // Placeholder
        )
    };

    fs::write(&cert_host_path, include_str!("fixtures/cert.pem")).unwrap();
    fs::write(&key_host_path, include_str!("fixtures/key.pem")).unwrap();

    // 2. Start TLS Backend
    let (backend_process, actual_upstream_port) = if mode == "docker" {
        println!("🐳 Starting backend-tls container...");
        let compose_file = project_root.join("crates/pavis-e2e/config/docker-compose.yaml");

        // Restart backend-tls to pick up new certs
        let _ = Command::new("docker")
            .args([
                "compose",
                "-f",
                compose_file.to_str().unwrap(),
                "restart",
                "backend-tls",
            ])
            .status()
            .expect("Failed to restart backend-tls");

        // If it wasn't running, restart might fail or do nothing?
        // Better to use `up -d --force-recreate`.
        let status = Command::new("docker")
            .args([
                "compose",
                "-f",
                compose_file.to_str().unwrap(),
                "up",
                "-d",
                "--force-recreate",
                "backend-tls",
            ])
            .status()
            .expect("Failed to start backend-tls");

        assert!(status.success());
        sleep(Duration::from_secs(2)).await;
        (None, upstream_port)
    } else {
        let port = {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        };

        println!("Starting openssl s_server on port {}", port);
        let child = Command::new("openssl")
            .arg("s_server")
            .arg("-cert")
            .arg(&cert_host_path)
            .arg("-key")
            .arg(&key_host_path)
            .arg("-accept")
            .arg(port.to_string())
            .arg("-www")
            // .arg("-quiet")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start openssl s_server");

        sleep(Duration::from_secs(2)).await;
        (Some(child), port)
    };

    // 3. Generate Config
    let pavis_port = if mode == "docker" {
        8080
    } else {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };

    let config_path = if mode == "docker" {
        project_root.join("crates/pavis-e2e/config/generated_upstream_tls.yaml")
    } else {
        std::env::temp_dir().join("pavis_test_upstream_tls/config.yaml")
    };

    let template = include_str!("../config/templates/upstream_tls.yaml");
    let config = template
        .replacen("{}", &pavis_port.to_string(), 1)
        .replacen("{}", &actual_upstream_port.to_string(), 1);

    // Patch upstream host for Docker
    let config = if mode == "docker" {
        config.replace("127.0.0.1", upstream_host)
    } else {
        config
    };

    fs::write(&config_path, config).unwrap();

    // 4. Start Pavis
    let mut pavis_process = None;
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
        sleep(Duration::from_secs(5)).await;
    } else {
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
        pavis_process = Some(child);
        sleep(Duration::from_secs(2)).await;
    }

    // 5. Make Request
    let client = Client::new();
    let url = format!("http://127.0.0.1:{}/", pavis_port);
    println!("Sending request to {}", url);

    let resp = client.get(&url).send().await;

    // 6. Cleanup
    if let Some(mut child) = pavis_process {
        let _ = child.kill();
    }
    if let Some(mut child) = backend_process {
        let _ = child.kill();
        // Print output
        if let Some(mut out) = child.stdout.take() {
            let mut s = String::new();
            use std::io::Read;
            out.read_to_string(&mut s).unwrap_or_default();
            println!("OpenSSL Stdout: {}", s);
        }
        if let Some(mut err) = child.stderr.take() {
            let mut s = String::new();
            use std::io::Read;
            err.read_to_string(&mut s).unwrap_or_default();
            println!("OpenSSL Stderr: {}", s);
        }
    }
    if mode == "binary" {
        let _ = fs::remove_dir_all(config_path.parent().unwrap());
    }

    // Assert
    match resp {
        Ok(r) => {
            assert!(
                r.status().is_success(),
                "Response was not success: {:?}",
                r.status()
            );
            let text = r.text().await.unwrap();
            println!("Response text: {}", text);
            // openssl s_server -www returns a page with "OpenSSL" or similar info
            assert!(
                text.to_lowercase().contains("openssl") || text.contains("s_server"),
                "Response did not look like openssl s_server output"
            );
        }
        Err(e) => {
            panic!("Request failed: {}", e);
        }
    }
}
