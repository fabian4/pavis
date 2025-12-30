use anyhow::{Context, Result};
use reqwest::Client;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

use super::config::{PavisScenario, generate_config};
use super::http::wait_for_pavis;

/// Test environment for E2E tests.
pub struct TestEnv {
    pavis_process: Option<Child>,
    config_path: PathBuf,
    pvs_path: Option<PathBuf>,
}

impl TestEnv {
    /// Creates a new test environment.
    ///
    /// # Errors
    /// Returns an error if config generation or process spawning fails.
    ///
    /// # Panics
    /// May panic if paths are not valid UTF-8.
    pub async fn new(scenario: PavisScenario) -> Result<Self> {
        let mode = env::var("TEST_MODE").unwrap_or_else(|_| "binary".to_string());
        let project_root = find_project_root()?;
        let config_dest = generate_config(&project_root, scenario, &mode)?;

        // 2. Start Pavis
        let mut process = None;
        let mut pvs_path = None;

        if mode == "binary" {
            let pavis_bin = find_binary(&project_root, "pavis")?;

            // If config is YAML, compile it to PVS
            let run_config_path = if config_dest
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                let pavctl_bin = find_binary(&project_root, "pavctl")?;
                let output_pvs = config_dest.with_extension("pvs");

                generate_pvs(&pavctl_bin, &config_dest, &output_pvs)?;
                pvs_path = Some(output_pvs.clone());
                output_pvs
            } else {
                config_dest.clone()
            };

            println!("🚀 Starting Pavis Binary ({run_config_path:?})...");
            let child = Command::new(&pavis_bin)
                .arg("--config")
                .arg(&run_config_path)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .context("Failed to spawn pavis")?;

            process = Some(child);
            sleep(Duration::from_secs(1)).await;
        } else if mode == "docker" {
            println!("🐳 Restarting Pavis Container with new config...");
            let shared_config = project_root.join("crates/pavis-e2e/config/generated_config.pvs");
            let run_config_path = if config_dest
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                let pavctl_bin = find_binary(&project_root, "pavctl")?;
                generate_pvs(&pavctl_bin, &config_dest, &shared_config)?;
                pvs_path = Some(shared_config.clone());
                shared_config.clone()
            } else {
                fs::copy(&config_dest, &shared_config)?;
                pvs_path = Some(shared_config.clone());
                shared_config.clone()
            };

            let compose_file = project_root.join("crates/pavis-e2e/config/docker-compose.yaml");

            let status = Command::new("docker")
                .args([
                    "compose",
                    "-f",
                    compose_file.to_str().expect("valid path"),
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
            let _ = run_config_path;
        }

        // Wait for health
        let client = Client::builder().timeout(Duration::from_secs(1)).build()?;
        wait_for_pavis(&client).await?;

        Ok(Self {
            pavis_process: process,
            config_path: config_dest,
            pvs_path,
        })
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if let Some(mut child) = self.pavis_process.take() {
            println!("🧹 Killing Pavis process...");
            let _ = child.kill();
            let _ = child.wait();
        }

        let _ = fs::remove_file(&self.config_path);
        if let Some(pvs) = &self.pvs_path {
            let _ = fs::remove_file(pvs);
        }
    }
}

pub fn find_binary(project_root: &Path, name: &str) -> Result<PathBuf> {
    let release_bin = project_root.join("target/release").join(name);
    if release_bin.exists() {
        return Ok(release_bin);
    }
    let debug_bin = project_root.join("target/debug").join(name);
    if debug_bin.exists() {
        return Ok(debug_bin);
    }
    Err(anyhow::anyhow!(
        "Binary '{name}' not found. Run cargo build."
    ))
}

pub fn generate_pvs(pavctl_bin: &PathBuf, input: &PathBuf, output: &PathBuf) -> Result<()> {
    println!("🔨 Generating PVS from YAML: {input:?} -> {output:?}");
    let status = Command::new(pavctl_bin)
        .arg("gen")
        .arg(input)
        .arg(output)
        .status()
        .context("Failed to run pavctl")?;

    if !status.success() {
        return Err(anyhow::anyhow!("Failed to generate config using pavctl"));
    }
    Ok(())
}

/// Finds the project root directory.
///
/// # Errors
/// Returns an error if Cargo.lock is not found.
pub fn find_project_root() -> Result<PathBuf> {
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
