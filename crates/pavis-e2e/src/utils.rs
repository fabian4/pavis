use anyhow::{Context, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

pub const BASE_URL: &str = "http://localhost:8080";

pub struct TestEnv {
    pavis_process: Option<Child>,
    config_path: PathBuf,
}

impl TestEnv {
    pub async fn new(config_name: &str) -> Result<Self> {
        let mode = env::var("TEST_MODE").unwrap_or_else(|_| "binary".to_string());
        let project_root = find_project_root()?;
        let config_src = project_root
            .join("crates/pavis-e2e/config/templates")
            .join(config_name);

        // We use a unique config name per test to avoid collision if run in parallel (future proofing)
        // though currently ports collide.
        let config_dest = project_root
            .join("crates/pavis-e2e/config")
            .join(format!("generated_{}", config_name));

        // 1. Generate Config
        let backend_v1 = env::var("BACKEND_V1_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let backend_v2 = env::var("BACKEND_V2_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        let content = fs::read_to_string(&config_src)
            .with_context(|| format!("Failed to read config: {:?}", config_src))?;

        let content = content
            .replace("${BACKEND_V1_HOST}", &backend_v1)
            .replace("${BACKEND_V2_HOST}", &backend_v2)
            .replace("${TEST_MODE}", &mode);

        fs::write(&config_dest, content)?;

        // 2. Start Pavis
        let mut process = None;

        if mode == "binary" {
            let binary_path = project_root.join("target/release/pavis");
            if !binary_path.exists() {
                return Err(anyhow::anyhow!(
                    "Pavis binary not found at {:?}. Run 'cargo build --release' first.",
                    binary_path
                ));
            }

            println!("🚀 Starting Pavis Binary ({:?})...", config_dest);
            let child = Command::new(&binary_path)
                .arg("--config")
                .arg(&config_dest)
                .stdout(Stdio::null()) // Reduce noise, or inherit for debug
                .stderr(Stdio::inherit())
                .spawn()
                .context("Failed to spawn pavis")?;

            process = Some(child);
            // Give it time to start
            sleep(Duration::from_secs(1)).await;
        } else if mode == "docker" {
            println!("🐳 Restarting Pavis Container with new config...");
            // For docker, we need to overwrite the standard generated_config.yaml
            // because that is what is mounted in docker-compose.
            let shared_config = project_root.join("crates/pavis-e2e/config/generated_config.yaml");
            fs::copy(&config_dest, &shared_config)?;

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
                .status()?;

            if !status.success() {
                return Err(anyhow::anyhow!("Failed to restart docker container"));
            }
            sleep(Duration::from_secs(2)).await;
        }

        // Wait for health
        let client = Client::builder().timeout(Duration::from_secs(1)).build()?;
        wait_for_pavis(&client).await?;

        Ok(Self {
            pavis_process: process,
            config_path: config_dest,
        })
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // Stop Binary
        if let Some(mut child) = self.pavis_process.take() {
            println!("🧹 Killing Pavis process...");
            let _ = child.kill();
            let _ = child.wait();
        }

        // Stop Docker (optional, or just leave it for next test to restart)
        // If we stop it here, it adds latency. Recreating it in new() is enough.

        // Cleanup config
        let _ = fs::remove_file(&self.config_path);
    }
}

fn find_project_root() -> Result<PathBuf> {
    let mut dir = env::current_dir()?;
    loop {
        if dir.join("Cargo.lock").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(anyhow::anyhow!(
                "Could not find project root (Cargo.lock not found)"
            ));
        }
    }
}

pub async fn wait_for_pavis(client: &Client) -> Result<()> {
    println!("🚀 Starting E2E Tests...");
    println!("Waiting for Pavis to be ready at {}...", BASE_URL);

    for _ in 0..30 {
        if client.get(BASE_URL).send().await.is_ok() {
            println!("✅ Pavis is up!");
            return Ok(());
        }
        print!(".");
        sleep(Duration::from_secs(1)).await;
    }

    Err(anyhow::anyhow!("❌ Timeout waiting for Pavis to start."))
}

pub async fn get_upstream_name(client: &Client, path: &str) -> Result<String> {
    let url = format!("{}{}", BASE_URL, path);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to send request")?;

    let status = resp.status();
    let text = resp.text().await.context("Failed to read response body")?;

    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "Request failed with status {}: {}",
            status,
            text
        ));
    }

    // Try to parse as generic JSON first
    if let Some(name) = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|json| {
            json.get("os")
                .and_then(|os| os.get("env"))
                .and_then(|env| env.get("SERVICE_NAME"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
    {
        return Ok(name);
    }

    // Fallback: Use string search if JSON structure varies
    if text.contains("backend-v1") {
        return Ok("backend-v1".to_string());
    }
    if text.contains("backend-v2") {
        return Ok("backend-v2".to_string());
    }

    Err(anyhow::anyhow!("Could not identify upstream from response"))
}

pub async fn get_response_json(
    client: &Client,
    path: &str,
    headers: HashMap<String, String>,
) -> Result<serde_json::Value> {
    let url = format!("{}{}", BASE_URL, path);
    let mut req = client.get(&url);
    for (k, v) in headers {
        req = req.header(k, v);
    }

    let resp = req.send().await.context("Failed to send request")?;
    let text = resp.text().await.context("Failed to read response body")?;

    serde_json::from_str(&text).context("Failed to parse JSON")
}
