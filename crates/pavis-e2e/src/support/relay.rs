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

#[derive(Debug, Clone)]
pub struct RelayOptions {
    pub enable_file_ingest: bool,
    pub ingest_debounce_ms: u64,
    pub ingest_path: Option<PathBuf>,
    pub lkg_path: Option<PathBuf>,
    pub storage_root: Option<PathBuf>,
    pub max_pvs_bytes: Option<u64>,
}

impl Default for RelayOptions {
    fn default() -> Self {
        Self {
            enable_file_ingest: true,
            ingest_debounce_ms: 500,
            ingest_path: None,
            lkg_path: None,
            storage_root: None,
            max_pvs_bytes: None,
        }
    }
}

pub struct RelayInstance {
    pub env: RelayEnv,
    pub lkg_path: PathBuf,
    pub ingest_path: Option<PathBuf>,
}

impl RelayInstance {
    #[allow(clippy::collapsible_if)]
    pub async fn new(options: RelayOptions) -> Result<Self> {
        let env = RelayEnv::new(options.clone()).await?;
        let lkg_path = env.lkg_path.clone();

        let ingest_path = if env.options.enable_file_ingest {
            Some(
                env.work_dir.join(
                    env.options
                        .ingest_path
                        .clone()
                        .unwrap_or_else(|| PathBuf::from("input.yaml")),
                ),
            )
        } else {
            None
        };

        if let Some(path) = ingest_path.as_ref() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            if !path.exists() {
                fs::write(
                    path,
                    "listeners:\n  - name: \"default\"\n    address: \"127.0.0.1:0\"",
                )?;
            }
        }

        let instance = Self {
            env,
            lkg_path,
            ingest_path,
        };

        if options.enable_file_ingest {
            let client = instance.client();
            for _ in 0..50 {
                if let Ok(status) = client.status().await {
                    if status.version >= 1 {
                        return Ok(instance);
                    }
                }
                sleep(Duration::from_millis(100)).await;
            }
            println!("WARN: Timeout waiting for initial version 1");
        }

        Ok(instance)
    }
    pub async fn restart(mut self) -> Result<Self> {
        self.env.restart().await?;
        Ok(self)
    }

    pub fn client(&self) -> RelayClient {
        RelayClient::new(self.env.base_url.clone())
    }
}

#[derive(Clone)]
pub struct RelayClient {
    base_url: String,
    inner: Client,
}

impl RelayClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            inner: Client::new(),
        }
    }

    pub async fn status(&self) -> Result<RelayStatus> {
        let resp = self
            .inner
            .get(format!("{}/v1/status", self.base_url))
            .send()
            .await?
            .error_for_status()?;
        let text = resp.text().await?;
        parse_relay_status(&text)
            .with_context(|| format!("Failed to parse status response: {text}"))
    }

    pub async fn metrics(&self) -> Result<String> {
        let preferred = format!("{}/v1/metrics", self.base_url);
        let fallback = format!("{}/metrics", self.base_url);
        let resp = self.inner.get(&preferred).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if status.is_success() {
            return Ok(text);
        }
        let resp = self.inner.get(&fallback).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Metrics request failed {}: {}",
                status,
                text
            ));
        }
        Ok(text)
    }

    pub async fn publish(&self, config: &pavis_core::RuntimeConfig) -> Result<PublishResponse> {
        let bytes = pavis_pvs::encode(config)?;
        self.publish_raw(bytes).await
    }

    pub async fn publish_raw(&self, bytes: Vec<u8>) -> Result<PublishResponse> {
        let proposed_version = self.next_version().await?;
        let resp = self
            .inner
            .post(format!("{}/v1/publish", self.base_url))
            .body(bytes)
            .header(
                pavis_pvs::PAVIS_VERSION_HEADER,
                proposed_version.to_string(),
            )
            .send()
            .await?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let text = resp.text().await?;

        if !status.is_success() {
            return Err(anyhow::anyhow!("Publish failed {}: {}", status, text));
        }

        parse_publish_response(&headers, &text)
            .with_context(|| format!("Failed to parse publish response: {text}"))
    }

    pub async fn get_artifact(&self, version: u64) -> Result<Vec<u8>> {
        let resp = self
            .inner
            .get(format!("{}/v1/artifacts/{}", self.base_url, version))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn long_poll(
        &self,
        current_version: u64,
        wait_ms: u64,
    ) -> Result<Option<(u64, Vec<u8>)>> {
        let resp = self
            .inner
            .get(format!("{}/v1/config", self.base_url))
            .query(&[("wait_ms", wait_ms)])
            .header(pavis_pvs::PAVIS_VERSION_HEADER, current_version.to_string())
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(None);
        }

        let resp = resp.error_for_status()?;

        let version_header = resp
            .headers()
            .get(pavis_pvs::PAVIS_VERSION_HEADER)
            .context("missing version header")?;
        let version_str = version_header.to_str()?;
        let version: u64 = version_str.parse()?;

        let bytes = resp.bytes().await?.to_vec();
        Ok(Some((version, bytes)))
    }

    async fn next_version(&self) -> Result<u64> {
        let status = self.status().await?;
        Ok(status.version.saturating_add(1))
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct RelayStatus {
    pub version: u64,
    pub checksum: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct PublishResponse {
    pub version: u64,
    pub checksum: String,
}

fn parse_relay_status(text: &str) -> Result<RelayStatus> {
    if text.trim_start().starts_with('{') {
        return serde_json::from_str(text).map_err(|err| err.into());
    }
    let mut version: Option<u64> = None;
    let mut checksum: Option<String> = None;
    for token in text.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        match key {
            "version" => {
                if let Ok(parsed) = value.parse::<u64>() {
                    version = Some(parsed);
                }
            }
            "checksum" => checksum = Some(value.to_string()),
            _ => {}
        }
    }
    let version = version.context("status missing version")?;
    let checksum = checksum.context("status missing checksum")?;
    Ok(RelayStatus { version, checksum })
}

fn parse_publish_response(
    headers: &reqwest::header::HeaderMap,
    text: &str,
) -> Result<PublishResponse> {
    if text.trim_start().starts_with('{') {
        return serde_json::from_str(text).map_err(|err| err.into());
    }
    let version = headers
        .get(pavis_pvs::PAVIS_VERSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .context("publish response missing version header")?;
    let checksum = headers
        .get(pavis_pvs::PAVIS_CHECKSUM_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .context("publish response missing checksum header")?;
    Ok(PublishResponse { version, checksum })
}

pub struct RelayEnv {
    child: Option<Child>,
    compose_project: Option<String>,
    compose_file: Option<PathBuf>,
    compose_shared: bool,
    base_url: String,
    config_path: PathBuf,
    pub work_dir: PathBuf,
    lkg_path: PathBuf,
    mode: TestMode,
    pub options: RelayOptions,
}

impl RelayEnv {
    pub async fn new(options: RelayOptions) -> Result<Self> {
        let mode = test_mode();
        let work_dir = unique_work_dir(&mode)?;

        // In docker mode with RELAY_PORT set, use that port; otherwise pick a random one
        let port = if matches!(mode, TestMode::Docker) {
            env::var("RELAY_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or_else(|| pick_port().unwrap_or(8083))
        } else {
            pick_port()?
        };

        let base_url = format!("http://127.0.0.1:{port}");

        let lkg_path = if let Some(path) = &options.lkg_path {
            path.clone()
        } else {
            work_dir.join("lkg").join("config.pvs")
        };

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
                let storage_root = if let Some(path) = &options.storage_root {
                    path.clone()
                } else {
                    work_dir.join("storage")
                };
                fs::create_dir_all(&storage_root)?;
                if let Some(parent) = lkg_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                (bind, storage_root, lkg_path.clone())
            }
            TestMode::Docker => {
                let bind = format!("0.0.0.0:{container_port}");
                let storage_root_host = if let Some(path) = &options.storage_root {
                    path.clone()
                } else {
                    work_dir.join("storage")
                };
                let lkg_dir_host = work_dir.join("lkg");

                create_dir_all_open(&storage_root_host)?;
                create_dir_all_open(&lkg_dir_host)?;

                let storage_root = PathBuf::from("/relay/storage");
                let lkg_path = PathBuf::from("/relay/lkg/config.pvs");
                (bind, storage_root, lkg_path)
            }
        };

        let container_ingest_path = match mode {
            TestMode::Docker => Some("/relay/input.yaml"),
            TestMode::Binary => None,
        };

        write_config(
            &config_path,
            &bind,
            &storage_root,
            &config_lkg_path,
            &options,
            container_ingest_path,
        )?;

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

        let mut env = Self {
            child,
            compose_project,
            compose_file,
            compose_shared,
            base_url,
            config_path,
            work_dir,
            lkg_path,
            mode,
            options,
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

    pub fn is_docker(&self) -> bool {
        matches!(self.mode, TestMode::Docker)
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

    async fn wait_for_ready(&mut self) -> Result<()> {
        let client = Client::builder().timeout(Duration::from_secs(1)).build()?;

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

        if let Some(status) = self
            .child
            .as_mut()
            .and_then(|c| c.try_wait().ok().flatten())
        {
            return Err(anyhow::anyhow!(
                "Relay process exited early with status: {}",
                status
            ));
        }

        Err(anyhow::anyhow!("relay did not become ready"))
    }
}

impl Drop for RelayEnv {
    fn drop(&mut self) {
        self.stop();
        // Only clean up work_dir if not using a shared directory
        if !self.compose_shared {
            let _ = fs::remove_dir_all(&self.work_dir);
        }
    }
}

fn unique_work_dir(mode: &TestMode) -> Result<PathBuf> {
    // If RELAY_WORK_DIR is set (for integrated tests), use it directly
    if let Ok(work_dir) = env::var("RELAY_WORK_DIR") {
        let path = PathBuf::from(&work_dir);
        fs::create_dir_all(&path)?;
        // Clean up stale state from previous tests
        // Use docker to remove files that may have been created by docker (as root)
        let storage_path = path.join("storage");
        let lkg_path = path.join("lkg");
        let input_path = path.join("input.yaml");

        let mut need_docker_cleanup = false;
        if storage_path.exists() && fs::remove_dir_all(&storage_path).is_err() {
            need_docker_cleanup = true;
        }
        if lkg_path.exists() && fs::remove_dir_all(&lkg_path).is_err() {
            need_docker_cleanup = true;
        }
        if input_path.exists() && fs::remove_file(&input_path).is_err() {
            need_docker_cleanup = true;
        }

        // Fallback to docker cleanup if direct removal failed (permission issues in CI)
        if need_docker_cleanup {
            let status = Command::new("docker")
                .args(["run", "--rm", "-v"])
                .arg(format!("{}:/work", path.display()))
                .args([
                    "alpine",
                    "rm",
                    "-rf",
                    "/work/storage",
                    "/work/lkg",
                    "/work/input.yaml",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();

            // Wait for cleanup to complete
            if let Ok(status) = status
                && !status.success()
            {
                return Err(anyhow::anyhow!(
                    "Docker cleanup failed with status: {}",
                    status
                ));
            }
        }

        // Ensure the cleanup actually happened before returning
        let storage_path = path.join("storage");
        let lkg_path = path.join("lkg");
        if storage_path.exists() {
            fs::remove_dir_all(&storage_path)?;
        }
        if lkg_path.exists() {
            fs::remove_dir_all(&lkg_path)?;
        }

        return Ok(path);
    }

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

pub fn pick_port() -> Result<u16> {
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
    options: &RelayOptions,
    container_ingest_path: Option<&str>,
) -> Result<()> {
    let max_bytes = options.max_pvs_bytes.unwrap_or(10485760);
    let mut content = format!(
        "identity:\n  name: relay-e2e\nhttp:\n  bind: \"{}\"\nstorage:\n  root_dir: \"{}\"\nartifact:\n  lkg_path: \"{}\"\n  limits:\n    max_pvs_bytes: {}
distribution:\n  long_poll:\n    enabled: true\n    headers:\n      version: \"X-Pavis-Version\"\n      checksum: \"X-Pavis-Checksum\"\n      algorithm: \"X-Pavis-Checksum-Alg\"\npersistence:\n  enabled: true\n  flush_interval: 100\n  retry:\n    max: 5\n    backoff:\n      min: 10\n      max: 100\n",
        bind,
        storage_root.display(),
        lkg_path.display(),
        max_bytes
    );

    if options.enable_file_ingest {
        let ingest_file_path = if let Some(container_path) = container_ingest_path {
            container_path.to_string()
        } else if let Some(p) = &options.ingest_path {
            p.display().to_string()
        } else {
            "input.yaml".to_string()
        };

        let pipeline_config = format!(
            "pipeline:\n  source_id: file:e2e\n  ingest:\n    source:\n      kind: file\n      path: \"{}\"\n    mode:\n      kind: watch\n      debounce: {}\n  codec:\n    kind: serde\n    mode:\n      compaction: off\n  runtime:\n    max_in_flight: 8\n    restart_backoff:\n      min: 100\n      max: 1000\n    publish_retry:\n      max: 3\n      backoff:\n        min: 10\n        max: 100\n",
            ingest_file_path, options.ingest_debounce_ms
        );
        content.push_str(&pipeline_config);
    }

    fs::write(config_path, content)?;
    Ok(())
}

fn spawn_relay(config_path: &Path) -> Result<Child> {
    let project_root = find_project_root()?;
    let relay_bin = find_binary(&project_root, "pavis-relay")?;

    let cwd = config_path.parent().unwrap();

    Command::new(&relay_bin)
        .env("RUST_LOG", "debug")
        .arg("--config")
        .arg(config_path)
        .current_dir(cwd)
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

    // Only set RELAY_PORT if not already in env (integrated tests pre-set it)
    let relay_port = env::var("RELAY_PORT").unwrap_or_else(|_| host_port.to_string());

    let status = Command::new("docker")
        .env("RELAY_IMAGE", image)
        .env("RELAY_PORT", relay_port)
        .env("RELAY_WORK_DIR", work_dir.display().to_string())
        .args([
            "compose",
            "-f",
            compose_file.to_str().expect("valid path"),
            "-p",
            &project,
            "up",
            "-d",
            "--force-recreate",
            "relay",
        ])
        .status()?;

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
