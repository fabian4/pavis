use anyhow::{Context, Result};
use reqwest::Client;
use std::env;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::sleep;

use super::pavis::find_project_root;

static COUNTER: AtomicU64 = AtomicU64::new(0);
const RELAY_CONTAINER_PORT: u16 = 8080;

pub struct RelayEnv {
    child: Option<Child>,
    compose_project: Option<String>,
    compose_file: Option<PathBuf>,
    compose_shared: bool,
    base_url: String,
    config_path: PathBuf,
    work_dir: PathBuf,
    lkg_path: PathBuf,
    mode: TestMode,
}

impl RelayEnv {
    pub async fn new() -> Result<Self> {
        let mode = test_mode();
        let work_dir = unique_work_dir(&mode)?;
        let port = pick_port()?;
        let base_url = format!("http://127.0.0.1:{port}");
        let lkg_path = work_dir.join("lkg").join("config.pvs");
        let config_path = work_dir.join("relay.yaml");
        let container_port = RELAY_CONTAINER_PORT;
        let project_root = if matches!(mode, TestMode::Docker) {
            Some(find_project_root()?)
        } else {
            None
        };
        let compose_override = env::var("RELAY_COMPOSE_FILE").ok();
        let compose_file = match (mode, compose_override) {
            (TestMode::Docker, Some(path)) => Some(PathBuf::from(path)),
            (TestMode::Docker, None) => project_root
                .as_ref()
                .map(|root| root.join("crates/pavis-e2e/config/docker-compose-relay.yaml")),
            _ => None,
        };

        let (bind, storage_root, config_lkg_path) = match mode {
            TestMode::Binary => {
                let bind = format!("127.0.0.1:{port}");
                let storage_root = work_dir.join("storage");
                fs::create_dir_all(&storage_root)?;
                (bind, storage_root, lkg_path.clone())
            }
            TestMode::Docker => {
                let bind = format!("0.0.0.0:{container_port}");
                let storage_root_host = work_dir.join("storage");
                let lkg_dir_host = work_dir.join("lkg");

                // Pre-create directories with wide permissions so host runner can delete/modify
                // files even if they are created by root inside the container.
                create_dir_all_open(&storage_root_host)?;
                create_dir_all_open(&lkg_dir_host)?;

                let storage_root = PathBuf::from("/relay/storage");
                let lkg_path = PathBuf::from("/relay/lkg/config.pvs");
                (bind, storage_root, lkg_path)
            }
        };

        write_config(&config_path, &bind, &storage_root, &config_lkg_path)?;

        let mut compose_shared = false;
        let (child, compose_project) = match mode {
            TestMode::Binary => (Some(spawn_relay(&config_path)?), None),
            TestMode::Docker => {
                let compose_file = compose_file
                    .as_ref()
                    .expect("compose file required in docker mode");
                let project_override = env::var("RELAY_COMPOSE_PROJECT").ok();
                if project_override.is_some() {
                    compose_shared = true;
                }
                let project =
                    spawn_relay_docker(compose_file, &work_dir, port, project_override.as_deref())?;
                (None, Some(project))
            }
        };

        let env = Self {
            child,
            compose_project,
            compose_file,
            compose_shared,
            base_url,
            config_path,
            work_dir,
            lkg_path,
            mode,
        };
        env.wait_for_ready().await?;
        Ok(env)
    }

    pub async fn restart(&mut self) -> Result<()> {
        self.stop();
        match self.mode {
            TestMode::Binary => {
                self.child = Some(spawn_relay(&self.config_path)?);
            }
            TestMode::Docker => {
                let port = self
                    .base_url
                    .rsplit(':')
                    .next()
                    .and_then(|value| value.parse::<u16>().ok())
                    .context("parse relay port")?;
                let compose_file = self
                    .compose_file
                    .as_ref()
                    .expect("compose file required in docker mode");
                self.compose_project = Some(spawn_relay_docker(
                    compose_file,
                    &self.work_dir,
                    port,
                    self.compose_project.as_deref(),
                )?);
            }
        }
        self.wait_for_ready().await
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn lkg_path(&self) -> &Path {
        &self.lkg_path
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let (Some(project), Some(compose_file)) =
            (self.compose_project.take(), self.compose_file.as_ref())
        {
            let action = if self.compose_shared { "stop" } else { "down" };
            let mut args = vec![
                "compose",
                "-f",
                compose_file.to_str().expect("valid path"),
                "-p",
                &project,
                action,
            ];
            if self.compose_shared {
                args.push("relay");
            }
            let _ = Command::new("docker")
                .args(&args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    async fn wait_for_ready(&self) -> Result<()> {
        let client = Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .context("build relay client")?;
        let url = format!("{}/health", self.base_url);

        for _ in 0..50 {
            if client
                .get(&url)
                .send()
                .await
                .map(|response| response.status().is_success())
                .unwrap_or(false)
            {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }

        Err(anyhow::anyhow!("relay did not become ready"))
    }
}

impl Drop for RelayEnv {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.work_dir);
    }
}

fn unique_work_dir(mode: &TestMode) -> Result<PathBuf> {
    let base_dir = match mode {
        TestMode::Binary => env::temp_dir(),
        TestMode::Docker => {
            let project_root = find_project_root()?;
            project_root
                .join("crates/pavis-e2e/config")
                .join("relay_tmp")
        }
    };
    fs::create_dir_all(&base_dir)?;
    let mut dir = base_dir;
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    dir.push(format!("pavis_relay_e2e_{pid}_{id}"));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn pick_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("bind relay port")?;
    let port = listener.local_addr().context("read relay port")?.port();
    drop(listener);
    Ok(port)
}

fn write_config(
    config_path: &Path,
    bind: &str,
    storage_root: &Path,
    lkg_path: &Path,
) -> Result<()> {
    let content = format!(
        "identity:\n  name: relay-e2e\nhttp:\n  bind: \"{bind}\"\nstorage:\n  root_dir: \"{}\"\nartifact:\n  lkg_path: \"{}\"\ndistribution:\n  long_poll:\n    enabled: true\n    headers:\n      version: \"X-Pavis-Version\"\n      checksum: \"X-Pavis-Checksum\"\n      algorithm: \"X-Pavis-Checksum-Alg\"\n",
        storage_root.display(),
        lkg_path.display()
    );
    fs::write(config_path, content)?;
    Ok(())
}

fn spawn_relay(config_path: &Path) -> Result<Child> {
    let project_root = find_project_root()?;
    let relay_bin = find_binary(&project_root, "pavis-relay")?;
    Command::new(&relay_bin)
        .arg("--config")
        .arg(config_path)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("spawn relay")
}

fn find_binary(project_root: &Path, name: &str) -> Result<PathBuf> {
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

fn spawn_relay_docker(
    compose_file: &Path,
    work_dir: &Path,
    host_port: u16,
    project_override: Option<&str>,
) -> Result<String> {
    let image = env::var("RELAY_IMAGE").unwrap_or_else(|_| "pavis-relay:ci".to_string());
    let project = project_override
        .map(|value| value.to_string())
        .unwrap_or_else(|| {
            env::var("RELAY_COMPOSE_PROJECT").unwrap_or_else(|_| {
                format!(
                    "pavis-relay-e2e-{}",
                    COUNTER.fetch_add(1, Ordering::Relaxed)
                )
            })
        });
    let status = Command::new("docker")
        .env("RELAY_IMAGE", image)
        .env("RELAY_PORT", host_port.to_string())
        .env("RELAY_WORK_DIR", work_dir.display().to_string())
        .args([
            "compose",
            "-f",
            compose_file.to_str().expect("valid path"),
            "-p",
            &project,
            "up",
            "-d",
            "relay",
        ])
        .status()
        .context("spawn relay container")?;

    if !status.success() {
        return Err(anyhow::anyhow!("Failed to start relay container"));
    }

    Ok(project)
}

fn test_mode() -> TestMode {
    match env::var("TEST_MODE")
        .unwrap_or_else(|_| "binary".to_string())
        .as_str()
    {
        "docker" => TestMode::Docker,
        _ => TestMode::Binary,
    }
}

#[derive(Clone, Copy)]
enum TestMode {
    Binary,
    Docker,
}

fn create_dir_all_open(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o777))?;
    }
    Ok(())
}
