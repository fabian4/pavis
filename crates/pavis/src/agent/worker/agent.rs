//! # ETag Invariant
//!
//! The runtime assumes identical `.pvs` artifact content MUST produce identical ETag values.
//!
//! ## Contract
//! - ETag = `sha256:<hex-digest>` computed over the full artifact bytes.
//! - Canonical form: `sha256:<lowercase-hex>` (no quotes, no "W/" prefix).
//! - Identical ETags imply byte-identical content.
//! - If the relay reuses an ETag for different content, the runtime will continue serving the
//!   previously validated artifact.
//!
//! ## Conditional Requests
//! - The runtime prefers `last_rejected_etag` over `last_applied_etag` for `If-None-Match`.
//! - This prevents repeated downloads of known-bad artifacts.
//! - The relay MUST return 304/204 when the conditional ETag matches its latest artifact.
//! - Returning 200 for a previously rejected ETag is a relay contract violation; the runtime logs
//!   an error and ignores the response.

use async_trait::async_trait;
use pingora::services::Service;
use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

use crate::state::{RuntimeState, RuntimeStateHandle};
use crate::telemetry::metrics::MetricsHandle;
use crate::validate_env::{self, RuntimeEnvError};

use crate::agent::backoff::Backoff;
use crate::agent::lkg::tmp_path_for;
use crate::agent::lkg::write_atomic;
use pavis_core::{CoreValidationError, RuntimeConfig};
use pavis_pvs::{PvsError, compute_checksum};

use pavis_core::{CONFIG_SIZE_HEADER, CONFIG_VERSION_HEADER, ETAG_HEADER};

type UpdateCallback = Box<dyn Fn(&RuntimeConfig) + Send + Sync>;

static CALLBACK_LOCK_POISONED: AtomicBool = AtomicBool::new(false);

pub struct ConfigAgent {
    relay_base: String,
    lkg_path: PathBuf,
    client: Client,
    backoff: Backoff,
    state: Arc<RuntimeStateHandle>,
    last_applied_etag: Arc<Mutex<Option<String>>>,
    last_rejected_etag: Arc<Mutex<Option<String>>>,
    on_update_callback: Mutex<Option<UpdateCallback>>,
    metrics: Arc<Mutex<Option<Arc<MetricsHandle>>>>,
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
            let conditional_etag = self.agent.get_conditional_etag();
            let wait_ms = if conditional_etag.is_some() {
                30_000
            } else {
                0
            };
            tokio::select! {
                _ = shutdown.changed() => break,
                result = self.agent.poll_once(wait_ms) => {
                    match result {
                        Ok(PollOutcome::Updated) => {
                            attempt = 0;
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                        Ok(PollOutcome::NoChange) => {
                            attempt = 0;
                            if wait_ms == 0 {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                            }
                        }
                        Ok(PollOutcome::Rejected) => {
                            attempt = 0;
                            // Do not clear rejected ETag here. We want to use it in the next poll
                            // to avoid re-downloading the same invalid artifact.
                            // If the relay still has this artifact, it will return 304.
                            // If the relay updates, it will return 200 with new content.
                            tokio::time::sleep(Duration::from_secs(5)).await;
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
            last_applied_etag: Arc::new(Mutex::new(None)),
            last_rejected_etag: Arc::new(Mutex::new(None)),
            on_update_callback: Mutex::new(None),
            metrics: Arc::new(Mutex::new(None)),
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

    pub fn set_metrics_handle(&self, handle: Arc<MetricsHandle>) {
        let mut guard = self
            .metrics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(handle);
    }

    #[doc(hidden)]
    pub fn new_for_tests(
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
            last_applied_etag: Arc::new(Mutex::new(None)),
            last_rejected_etag: Arc::new(Mutex::new(None)),
            on_update_callback: Mutex::new(None),
            metrics: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn poll_once(&self, wait_ms: u64) -> anyhow::Result<PollOutcome> {
        let conditional_etag = self.get_conditional_etag();
        let url = format!("{}/v1/config?wait_ms={wait_ms}", self.relay_base);
        let mut request = self.client.get(url);
        if let Some(etag) = conditional_etag.as_deref() {
            request = request.header("if-none-match", format!("\"{etag}\""));
        }
        let response = request.send().await?;

        match response.status().as_u16() {
            200 => {
                let header_etag = response
                    .headers()
                    .get(ETAG_HEADER)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| anyhow::anyhow!("missing {ETAG_HEADER} response header"))?;
                let header_etag = parse_etag_header(header_etag)?;

                let config_version = response
                    .headers()
                    .get(CONFIG_VERSION_HEADER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(parse_config_version_header);
                let config_size = response
                    .headers()
                    .get(CONFIG_SIZE_HEADER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok());

                if self.is_etag_current(&header_etag) {
                    self.record_config_stats(config_version, config_size, "current etag");
                    tracing::debug!(etag = header_etag, "config etag unchanged, skipping update");
                    return Ok(PollOutcome::NoChange);
                }

                if self.is_etag_rejected(&header_etag) {
                    self.record_config_stats(
                        config_version,
                        config_size,
                        "rejected etag (relay violation)",
                    );
                    tracing::error!(
                        etag = header_etag,
                        "relay returned 200 for previously rejected ETag; expected 304 (relay contract violation)"
                    );
                    return Ok(PollOutcome::NoChange);
                }

                let bytes = response.bytes().await?;
                match self
                    .apply_update(bytes.to_vec(), header_etag.clone(), config_version)
                    .await
                {
                    Ok(()) => Ok(PollOutcome::Updated),
                    Err(err) => {
                        self.set_last_rejected_etag(header_etag);
                        tracing::warn!(
                            error = %err,
                            "config validation failed; continuing with LKG"
                        );
                        Ok(PollOutcome::Rejected)
                    }
                }
            }
            204 | 304 => Ok(PollOutcome::NoChange),
            status => Err(anyhow::anyhow!("poll failed: status={status}")),
        }
    }

    async fn apply_update(
        &self,
        bytes: Vec<u8>,
        expected_etag: String,
        config_version: Option<u64>,
    ) -> anyhow::Result<()> {
        let actual_etag = checksum_for_bytes(&bytes);
        if actual_etag != expected_etag {
            return Err(self.record_validation_failure(anyhow::anyhow!(
                "etag/sha256 mismatch: expected={}, computed={}",
                expected_etag,
                actual_etag
            )));
        }
        pavis_pvs::verify(&bytes).map_err(|err| self.record_validation_failure(err.into()))?;

        let tmp_path = tmp_path_for(&self.lkg_path);
        write_atomic(&tmp_path, &bytes)
            .await
            .map_err(|err| self.record_apply_failure(err))?;

        let config = match pavis_pvs::load(&tmp_path) {
            Ok(config) => config,
            Err(err) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(self.record_validation_failure(err.into()));
            }
        };
        // SAFETY: agent receives `.pvs` artifacts which are canonically validated.
        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
        let current = self.state.load();
        if let Err(err) = validate_env::validate_runtime_env(&validated, Some(&current.config)) {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(self.record_validation_failure(err.into()));
        }
        let mut state = RuntimeState::from_config(&validated)
            .map_err(|err| self.record_validation_failure(err))?;
        state.config_version = config_version;

        self.record_validation("ok", "none");
        tracing::info!(
            target: "pavis.config",
            event = "config_validation",
            result = "ok",
            reason = "none",
            etag = expected_etag
        );

        tokio::fs::rename(&tmp_path, &self.lkg_path)
            .await
            .map_err(|err| self.record_apply_failure(err.into()))?;

        self.state.store(state);
        self.set_last_applied_etag(expected_etag.clone());
        self.clear_last_rejected_etag();
        self.record_config_stats(config_version, Some(bytes.len() as u64), "applied update");
        if let (Some(handle), Some(version)) = (self.metrics_handle(), config_version) {
            let current = self.state.load();
            if current.config_version == Some(version) {
                handle.increment_reload_count();
            }
        }

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

        self.record_apply("ok");
        tracing::info!(
            target: "pavis.config",
            event = "config_apply",
            result = "ok",
            etag = expected_etag
        );
        Ok(())
    }

    fn metrics_handle(&self) -> Option<Arc<MetricsHandle>> {
        self.metrics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn record_validation(&self, result: &str, reason: &str) {
        if let Some(handle) = self.metrics_handle() {
            handle.record_config_validation(result, reason);
        }
    }

    fn record_apply(&self, result: &str) {
        if let Some(handle) = self.metrics_handle() {
            handle.record_config_apply(result);
        }
    }

    fn record_config_stats(&self, version: Option<u64>, size_bytes: Option<u64>, reason: &str) {
        let (Some(handle), Some(version), Some(size_bytes)) =
            (self.metrics_handle(), version, size_bytes)
        else {
            tracing::debug!(reason, "skipping config stats update");
            return;
        };
        handle.update_config_stats(&version.to_string(), size_bytes);
    }

    fn record_validation_failure(&self, err: anyhow::Error) -> anyhow::Error {
        let reason = classify_validation_error(&err);
        self.record_validation("fail", reason);
        self.record_apply("fail");
        tracing::warn!(
            target: "pavis.config",
            event = "config_validation",
            result = "fail",
            reason,
            error = %err
        );
        tracing::warn!(
            target: "pavis.config",
            event = "config_apply",
            result = "fail",
            error = %err
        );
        err
    }

    fn record_apply_failure(&self, err: anyhow::Error) -> anyhow::Error {
        self.record_apply("fail");
        tracing::warn!(
            target: "pavis.config",
            event = "config_apply",
            result = "fail",
            error = %err
        );
        err
    }

    #[doc(hidden)]
    pub async fn apply_update_for_tests(
        &self,
        bytes: Vec<u8>,
        etag: String,
        version: Option<u64>,
    ) -> anyhow::Result<()> {
        self.apply_update(bytes, etag, version).await
    }

    #[doc(hidden)]
    pub fn last_applied_etag_for_tests(&self) -> Option<String> {
        let guard = self
            .last_applied_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clone()
    }

    #[doc(hidden)]
    pub fn set_last_applied_etag_for_tests(&self, value: Option<String>) {
        let mut guard = self
            .last_applied_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = value;
    }

    #[doc(hidden)]
    pub fn last_rejected_etag_for_tests(&self) -> Option<String> {
        let guard = self
            .last_rejected_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clone()
    }

    #[doc(hidden)]
    pub fn set_last_rejected_etag_for_tests(&self, value: Option<String>) {
        let mut guard = self
            .last_rejected_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = value;
    }
}

#[derive(Debug)]
pub enum PollOutcome {
    Updated,
    NoChange,
    Rejected,
}

fn classify_validation_error(err: &anyhow::Error) -> &'static str {
    if let Some(pvs_err) = err.downcast_ref::<PvsError>() {
        return match pvs_err {
            PvsError::VersionMismatch { .. } => "version",
            _ => "parse",
        };
    }
    if err.downcast_ref::<RuntimeEnvError>().is_some() {
        return "runtime";
    }
    if err.downcast_ref::<CoreValidationError>().is_some() {
        return "semantic";
    }
    if err.to_string().contains("etag/sha256 mismatch") {
        return "parse";
    }
    "semantic"
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

fn parse_config_version_header(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok()
}

fn parse_etag_header(value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();

    if trimmed.starts_with("W/") || trimmed.starts_with("w/") {
        anyhow::bail!("weak ETags not supported: {value}");
    }

    let unquoted = if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    if !unquoted.starts_with("sha256:") {
        anyhow::bail!("invalid etag format (expected sha256:...): {value}");
    }
    let hex = &unquoted["sha256:".len()..];
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("invalid etag format (expected 64 hex chars): {value}");
    }

    Ok(format!("sha256:{}", hex.to_lowercase()))
}

impl ConfigAgent {
    fn is_etag_current(&self, etag: &str) -> bool {
        let guard = self
            .last_applied_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.as_deref() == Some(etag)
    }

    fn set_last_applied_etag(&self, etag: String) {
        let mut guard = self
            .last_applied_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(etag);
    }

    fn get_conditional_etag(&self) -> Option<String> {
        let rejected = self
            .last_rejected_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if rejected.is_some() {
            return rejected;
        }
        self.last_applied_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn is_etag_rejected(&self, etag: &str) -> bool {
        let guard = self
            .last_rejected_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.as_deref() == Some(etag)
    }

    fn set_last_rejected_etag(&self, etag: String) {
        let mut guard = self
            .last_rejected_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(etag);
    }

    fn clear_last_rejected_etag(&self) {
        let mut guard = self
            .last_rejected_etag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }
}
