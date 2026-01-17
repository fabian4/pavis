use async_trait::async_trait;
use pingora::services::Service;
use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

use crate::state::{RuntimeState, RuntimeStateHandle};

use crate::agent::backoff::Backoff;
use crate::agent::lkg::tmp_path_for;
use crate::agent::lkg::write_atomic;
use pavis_core::RuntimeConfig;
use pavis_pvs::compute_checksum;

const ETAG_HEADER: &str = "etag";

type UpdateCallback = Box<dyn Fn(&RuntimeConfig) + Send + Sync>;

static CALLBACK_LOCK_POISONED: AtomicBool = AtomicBool::new(false);

pub struct ConfigAgent {
    relay_base: String,
    lkg_path: PathBuf,
    client: Client,
    backoff: Backoff,
    state: Arc<RuntimeStateHandle>,
    last_checksum: Arc<Mutex<Option<String>>>,
    on_update_callback: Mutex<Option<UpdateCallback>>,
}

pub struct ConfigAgentWorker {
    agent: Arc<ConfigAgent>,
}

#[async_trait]
impl Service for ConfigAgentWorker {
    async fn start_service(
        &mut self,
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: watch::Receiver<bool>,
        _threads: usize,
    ) {
        let mut attempt = 0u32;
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                result = self.agent.poll_once() => {
                    match result {
                        Ok(PollOutcome::Updated) | Ok(PollOutcome::NoChange) => {
                            attempt = 0;
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "config poll failed");
                            let delay = self.agent.backoff.next_delay(attempt);
                            attempt = attempt.saturating_add(1);
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            }
        }
    }

    fn name(&self) -> &str {
        "config_poller"
    }
}

impl ConfigAgent {
    pub fn new(
        relay_base: String,
        lkg_path: PathBuf,
        state: Arc<RuntimeStateHandle>,
        timeout: Duration,
        backoff: Backoff,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self {
            relay_base,
            lkg_path,
            client,
            backoff,
            state,
            last_checksum: Arc::new(Mutex::new(None)),
            on_update_callback: Mutex::new(None),
        })
    }

    pub fn on_update<F>(&self, callback: F)
    where
        F: Fn(&RuntimeConfig) + Send + Sync + 'static,
    {
        let mut guard = match self.on_update_callback.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                if !CALLBACK_LOCK_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("on_update callback lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        *guard = Some(Box::new(callback));
    }

    pub fn worker(self: Arc<Self>) -> ConfigAgentWorker {
        ConfigAgentWorker { agent: self }
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(
        relay_base: String,
        lkg_path: PathBuf,
        state: Arc<RuntimeStateHandle>,
        client: Client,
        backoff: Backoff,
    ) -> Self {
        Self {
            relay_base,
            lkg_path,
            client,
            backoff,
            state,
            last_checksum: Arc::new(Mutex::new(None)),
            on_update_callback: Mutex::new(None),
        }
    }

    pub async fn poll_once(&self) -> anyhow::Result<PollOutcome> {
        let current_checksum = {
            let guard = self
                .last_checksum
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.clone()
        };
        let wait_ms = if current_checksum.is_some() { 30000 } else { 0 };
        let url = format!("{}/v1/config?wait_ms={wait_ms}", self.relay_base);
        let mut request = self.client.get(url);
        if let Some(checksum) = current_checksum.as_deref() {
            request = request.header("if-none-match", format!("\"{checksum}\""));
        }
        let response = request.send().await?;

        match response.status().as_u16() {
            200 => {
                let header_etag = response
                    .headers()
                    .get(ETAG_HEADER)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| anyhow::anyhow!("missing {ETAG_HEADER} response header"))?;
                let header_checksum = parse_etag_header(header_etag)?;

                if self.is_checksum_current(&header_checksum) {
                    tracing::debug!(
                        checksum = header_checksum,
                        "config checksum unchanged, skipping update"
                    );
                    return Ok(PollOutcome::NoChange);
                }

                let bytes = response.bytes().await?;
                self.apply_update(bytes.to_vec(), header_checksum).await?;
                Ok(PollOutcome::Updated)
            }
            204 | 304 => Ok(PollOutcome::NoChange),
            status => Err(anyhow::anyhow!("poll failed: status={status}")),
        }
    }

    async fn apply_update(&self, bytes: Vec<u8>, expected_checksum: String) -> anyhow::Result<()> {
        let actual_checksum = checksum_for_bytes(&bytes);
        if actual_checksum != expected_checksum {
            anyhow::bail!(
                "checksum mismatch: expected={}, computed={}",
                expected_checksum,
                actual_checksum
            );
        }
        let _ = pavis_pvs::verify(&bytes)?;

        let tmp_path = tmp_path_for(&self.lkg_path);
        write_atomic(&tmp_path, &bytes).await?;

        let config = match pavis_pvs::load(&tmp_path) {
            Ok(config) => config,
            Err(err) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(err.into());
            }
        };
        // SAFETY: agent receives `.pvs` artifacts which are canonically validated.
        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
        let state = RuntimeState::from_config(&validated)?;

        tokio::fs::rename(&tmp_path, &self.lkg_path).await?;

        self.state.store(state);
        self.set_last_checksum(actual_checksum);

        let callback = match self.on_update_callback.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                if !CALLBACK_LOCK_POISONED.swap(true, Ordering::Relaxed) {
                    tracing::warn!("on_update callback lock was poisoned; recovering");
                }
                poisoned.into_inner()
            }
        };
        if let Some(callback) = callback.as_ref() {
            callback(&validated);
        }

        tracing::info!(checksum = expected_checksum, "Applied configuration update");
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn apply_update_for_tests(
        &self,
        bytes: Vec<u8>,
        checksum: String,
    ) -> anyhow::Result<()> {
        self.apply_update(bytes, checksum).await
    }

    #[cfg(test)]
    pub(crate) fn last_checksum_for_tests(&self) -> Option<String> {
        let guard = self
            .last_checksum
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clone()
    }

    #[cfg(test)]
    pub(crate) fn set_last_checksum_for_tests(&self, value: Option<String>) {
        let mut guard = self
            .last_checksum
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = value;
    }
}

#[derive(Debug)]
pub enum PollOutcome {
    Updated,
    NoChange,
}

fn checksum_for_bytes(bytes: &[u8]) -> String {
    let digest = compute_checksum(bytes);
    let mut out = String::with_capacity(digest.len() * 2 + "sha256:".len());
    out.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn parse_etag_header(value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    if !trimmed.starts_with('"') || !trimmed.ends_with('"') || trimmed.len() < 2 {
        anyhow::bail!("invalid etag format: {value}");
    }
    let unquoted = &trimmed[1..trimmed.len() - 1];
    if !unquoted.starts_with("sha256:") {
        anyhow::bail!("invalid etag format: {value}");
    }
    let hex = &unquoted["sha256:".len()..];
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("invalid etag format: {value}");
    }
    Ok(format!("sha256:{}", hex.to_lowercase()))
}

impl ConfigAgent {
    fn is_checksum_current(&self, checksum: &str) -> bool {
        let guard = self
            .last_checksum
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.as_deref() == Some(checksum)
    }

    fn set_last_checksum(&self, checksum: String) {
        let mut guard = self
            .last_checksum
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(checksum);
    }
}
