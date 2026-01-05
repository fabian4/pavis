use async_trait::async_trait;
use pingora::services::Service;
use reqwest::Client;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::watch;

use crate::state::{RuntimeState, RuntimeStateHandle};

use crate::agent::backoff::Backoff;
use crate::agent::lkg::{tmp_path_for, version_path_for, write_atomic, write_version};
use pavis_pvs::PAVIS_VERSION_HEADER;

pub struct ConfigAgent {
    relay_base: String,
    lkg_path: PathBuf,
    version_path: PathBuf,
    client: Client,
    backoff: Backoff,
    state: Arc<RuntimeStateHandle>,
    current_version: AtomicU64,
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
        let version_path = version_path_for(&lkg_path);
        let client = Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self {
            relay_base,
            lkg_path,
            version_path,
            client,
            backoff,
            state,
            current_version: AtomicU64::new(0),
        })
    }

    pub fn worker(self: Arc<Self>) -> ConfigAgentWorker {
        ConfigAgentWorker { agent: self }
    }

    pub fn set_current_version(&self, version: u64) {
        self.current_version.store(version, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(
        relay_base: String,
        lkg_path: PathBuf,
        state: Arc<RuntimeStateHandle>,
        client: Client,
        backoff: Backoff,
        current_version: u64,
    ) -> Self {
        let version_path = version_path_for(&lkg_path);
        Self {
            relay_base,
            lkg_path,
            version_path,
            client,
            backoff,
            state,
            current_version: AtomicU64::new(current_version),
        }
    }

    pub async fn poll_once(&self) -> anyhow::Result<PollOutcome> {
        let version = self.current_version.load(Ordering::SeqCst);
        let url = format!("{}/v1/config", self.relay_base);
        let response = self
            .client
            .get(url)
            .header(PAVIS_VERSION_HEADER, version.to_string())
            .send()
            .await?;

        match response.status().as_u16() {
            200 => {
                let header_version = response
                    .headers()
                    .get(PAVIS_VERSION_HEADER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or_else(|| {
                        anyhow::anyhow!("missing {PAVIS_VERSION_HEADER} response header")
                    })?;

                if header_version <= version {
                    tracing::warn!(
                        current = version,
                        received = header_version,
                        "received stale config version, ignoring"
                    );
                    return Ok(PollOutcome::NoChange);
                }

                let bytes = response.bytes().await?;
                self.apply_update(bytes.to_vec(), header_version).await?;
                Ok(PollOutcome::Updated)
            }
            204 | 304 => Ok(PollOutcome::NoChange),
            status => Err(anyhow::anyhow!("poll failed: status={status}")),
        }
    }

    async fn apply_update(&self, bytes: Vec<u8>, version: u64) -> anyhow::Result<()> {
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
        let validated = crate::load::assume_validated(config);
        let state = RuntimeState::from_config(&validated)?;

        tokio::fs::rename(&tmp_path, &self.lkg_path).await?;
        if let Err(err) = write_version(&self.version_path, version).await {
            tracing::warn!(error = %err, "failed to persist LKG version metadata");
        }

        self.state.store(state);
        self.current_version.store(version, Ordering::SeqCst);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn apply_update_for_tests(
        &self,
        bytes: Vec<u8>,
        version: u64,
    ) -> anyhow::Result<()> {
        self.apply_update(bytes, version).await
    }

    #[cfg(test)]
    pub(crate) fn current_version_for_tests(&self) -> u64 {
        self.current_version.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
pub enum PollOutcome {
    Updated,
    NoChange,
}
