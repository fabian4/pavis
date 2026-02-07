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

use async_trait::async_trait;
use pingora::services::Service;
use reqwest::Client;
use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

use crate::agent::fsm::{Effect, Event, Fsm, Response, VerifiedUpdate};
use crate::agent::lkg::{load_lkg_config, tmp_path_for, write_atomic};
use crate::state::{RuntimeState, RuntimeStateHandle};
use crate::telemetry::metrics::MetricsRegistry;
use crate::validate_env::{self, RuntimeEnvError};

use pavis_core::{
    CONFIG_SIZE_HEADER, CONFIG_VERSION_HEADER, ConfigVersion, CoreValidationError, ETAG_HEADER,
    RuntimeConfig,
};
use pavis_pvs::{PvsError, compute_checksum};

use crate::agent::fsm::REJECT_TTL;

type UpdateCallback = Box<dyn Fn(&RuntimeConfig) + Send + Sync>;

static CALLBACK_LOCK_POISONED: AtomicBool = AtomicBool::new(false);

pub struct ConfigAgent {
    relay_base: String,
    lkg_path: PathBuf,
    client: Client,
    state: Arc<RuntimeStateHandle>,
    on_update_callback: Mutex<Option<UpdateCallback>>,
    metrics: Arc<Mutex<Option<Arc<MetricsRegistry>>>>,
    fsm: Mutex<Fsm>,
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
        let mut event_queue = VecDeque::new();

        if let Err(err) = self.bootstrap_from_lkg().await {
            tracing::warn!(error = %err, "failed to load local LKG; continuing");
        }

        event_queue.push_back(Event::Start {
            now: std::time::Instant::now(),
        });
        self.run_loop(&mut shutdown, event_queue).await;
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
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(5))
            .build()?;
        let fsm = Mutex::new(Fsm::new_with_lkg_path(lkg_path.clone()));
        Ok(Self {
            relay_base,
            lkg_path,
            client,
            state,
            on_update_callback: Mutex::new(None),
            metrics: Arc::new(Mutex::new(None)),
            fsm,
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

    pub fn current_state(&self) -> crate::agent::fsm::StateSummary {
        let guard = self
            .fsm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.current_state()
    }

    pub fn set_metrics_handle(&self, handle: Arc<MetricsRegistry>) {
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
    ) -> Self {
        let fsm = Mutex::new(Fsm::new_with_lkg_path(lkg_path.clone()));
        Self {
            relay_base,
            lkg_path,
            client,
            state,
            on_update_callback: Mutex::new(None),
            metrics: Arc::new(Mutex::new(None)),
            fsm,
        }
    }

    #[doc(hidden)]
    pub fn last_applied_etag_for_tests(&self) -> Option<String> {
        let guard = self
            .fsm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.context().last_applied_etag.clone()
    }

    #[doc(hidden)]
    pub fn set_last_applied_etag_for_tests(&self, value: Option<String>) {
        let mut guard = self
            .fsm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.context_mut().last_applied_etag = value;
    }

    #[doc(hidden)]
    pub fn last_rejected_etag_for_tests(&self) -> Option<String> {
        let guard = self
            .fsm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.context().last_rejected_etag.clone()
    }

    #[doc(hidden)]
    pub fn set_last_rejected_etag_for_tests(&self, value: Option<String>) {
        let mut guard = self
            .fsm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.context_mut().last_rejected_etag = value;
    }

    #[doc(hidden)]
    pub fn set_last_rejected_etag_with_ttl_for_tests(&self, etag: String) {
        let mut guard = self
            .fsm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.context_mut().last_rejected_etag = Some(etag);
        guard.context_mut().last_rejected_until = Some(std::time::Instant::now() + REJECT_TTL);
    }

    #[doc(hidden)]
    pub async fn apply_update_for_tests(
        &self,
        bytes: Vec<u8>,
        etag: String,
        version: Option<ConfigVersion>,
    ) -> anyhow::Result<()> {
        let update = self
            .verify_update(bytes, etag.clone(), version, None)
            .await?;
        let (etag, version) = self.apply_update(update).await?;
        let mut guard = self
            .fsm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Only save ETag if not in forced unconditional mode (after 410 resync)
        if guard.context_mut().force_unconditional_count == 0 {
            guard.context_mut().last_applied_etag = Some(etag);
        }
        guard.context_mut().last_rejected_etag = None;
        guard.context_mut().last_rejected_until = None;
        guard.context_mut().backoff_attempt = 0;
        guard.context_mut().observed_version = version;
        Ok(())
    }

    #[doc(hidden)]
    pub async fn poll_once(&self, wait_ms: u64) -> anyhow::Result<PollOutcome> {
        let now = std::time::Instant::now();
        let conditional = {
            let mut guard = self
                .fsm
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let _ = guard.tick(Event::Start { now });
            guard.context_mut().conditional_etag(now)
        };

        let response_event = self.fetch_once(wait_ms, conditional).await?;
        let mut effects = {
            let mut guard = self
                .fsm
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.tick(response_event)
        };

        let mut outcome = PollOutcome::NoChange;
        while let Some(effect) = effects.pop() {
            match effect {
                Effect::Verify {
                    etag,
                    version,
                    size,
                    bytes,
                } => match self.verify_update(bytes, etag.clone(), version, size).await {
                    Ok(update) => {
                        let mut guard = self
                            .fsm
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        effects.extend(guard.tick(Event::VerifyOk {
                            update,
                            now: std::time::Instant::now(),
                        }));
                    }
                    Err(_err) => {
                        outcome = PollOutcome::Rejected;
                        let mut guard = self
                            .fsm
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        effects.extend(guard.tick(Event::VerifyFail {
                            etag,
                            now: std::time::Instant::now(),
                        }));
                        return Ok(outcome);
                    }
                },
                Effect::Apply { update } => {
                    let update_etag = update.etag.clone();
                    match self.apply_update(update).await {
                        Ok((etag, version)) => {
                            outcome = PollOutcome::Updated;
                            let mut guard = self
                                .fsm
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            effects.extend(guard.tick(Event::ApplyOk {
                                etag,
                                version,
                                now: std::time::Instant::now(),
                            }));
                        }
                        Err(_err) => {
                            outcome = PollOutcome::Rejected;
                            let mut guard = self
                                .fsm
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            effects.extend(guard.tick(Event::ApplyFail {
                                etag: update_etag,
                                now: std::time::Instant::now(),
                            }));
                            return Ok(outcome);
                        }
                    }
                }
                Effect::DiscardTemp { path } => {
                    let _ = tokio::fs::remove_file(path).await;
                }
                Effect::FetchConditional { .. } | Effect::FetchUnconditional { .. } => {}
                Effect::ScheduleTimer { .. } => {}
            }
        }

        Ok(outcome)
    }

    fn metrics_handle(&self) -> Option<Arc<MetricsRegistry>> {
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

    fn record_config_stats(
        &self,
        version: Option<ConfigVersion>,
        size_bytes: Option<u64>,
        reason: &str,
    ) {
        let (Some(handle), Some(version), Some(size_bytes)) =
            (self.metrics_handle(), version, size_bytes)
        else {
            tracing::debug!(reason, "skipping config stats update");
            return;
        };
        let version_label = version.to_string();
        handle.update_config_stats(&version_label, size_bytes);
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
}

impl ConfigAgentWorker {
    async fn bootstrap_from_lkg(&self) -> anyhow::Result<()> {
        if !self.agent.lkg_path.exists() {
            tracing::debug!("bootstrap_from_lkg: LKG path does not exist, skipping");
            return Ok(());
        }

        let bytes = tokio::fs::read(&self.agent.lkg_path).await?;
        pavis_pvs::verify(&bytes)?;
        let config = load_lkg_config(&self.agent.lkg_path)?.0;

        let current = self.agent.state.load();
        validate_env::validate_runtime_env(&config, Some(&current.config))?;

        let mut state = RuntimeState::from_config(&config)?;
        state.config_version = current.config_version;

        self.agent.state.store(state);

        let etag = checksum_for_bytes(&bytes);
        tracing::info!(
            etag = etag,
            lkg_path = ?self.agent.lkg_path,
            "bootstrap_from_lkg: setting initial last_applied_etag from LKG"
        );
        let mut guard = self
            .agent
            .fsm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.context_mut().last_applied_etag = Some(etag);

        Ok(())
    }

    async fn run_loop(
        &self,
        shutdown: &mut watch::Receiver<bool>,
        mut event_queue: VecDeque<Event>,
    ) {
        let mut pending_fetch: Option<tokio::task::JoinHandle<Event>> = None;
        let mut pending_timer: Option<Pin<Box<tokio::time::Sleep>>> = None;

        loop {
            while let Some(event) = event_queue.pop_front() {
                let effects = {
                    let mut guard = self
                        .agent
                        .fsm
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    guard.tick(event)
                };
                for effect in effects {
                    match effect {
                        Effect::FetchConditional { wait_ms, etag } => {
                            if pending_fetch.is_none() {
                                let agent = Arc::clone(&self.agent);
                                pending_fetch = Some(tokio::spawn(async move {
                                    fetch_effect(agent, Some(etag), wait_ms).await
                                }));
                            }
                        }
                        Effect::FetchUnconditional { wait_ms } => {
                            if pending_fetch.is_none() {
                                let agent = Arc::clone(&self.agent);
                                pending_fetch = Some(tokio::spawn(async move {
                                    fetch_effect(agent, None, wait_ms).await
                                }));
                            }
                        }
                        Effect::Verify {
                            etag,
                            version,
                            size,
                            bytes,
                        } => {
                            match self
                                .agent
                                .verify_update(bytes, etag.clone(), version, size)
                                .await
                            {
                                Ok(update) => event_queue.push_back(Event::VerifyOk {
                                    update,
                                    now: std::time::Instant::now(),
                                }),
                                Err(err) => {
                                    tracing::warn!(error = %err, "config verification failed");
                                    event_queue.push_back(Event::VerifyFail {
                                        etag,
                                        now: std::time::Instant::now(),
                                    });
                                }
                            }
                        }
                        Effect::Apply { update } => {
                            match self.agent.apply_update(update.clone()).await {
                                Ok((etag, version)) => event_queue.push_back(Event::ApplyOk {
                                    etag,
                                    version,
                                    now: std::time::Instant::now(),
                                }),
                                Err(err) => {
                                    tracing::warn!(error = %err, "config apply failed");
                                    event_queue.push_back(Event::ApplyFail {
                                        etag: update.etag,
                                        now: std::time::Instant::now(),
                                    });
                                }
                            }
                        }
                        Effect::ScheduleTimer { duration } => {
                            pending_timer = Some(Box::pin(tokio::time::sleep(duration)));
                        }
                        Effect::DiscardTemp { path } => {
                            let _ = tokio::fs::remove_file(path).await;
                        }
                    }
                }
            }

            tokio::select! {
                _ = shutdown.changed() => {
                    let mut guard = self.agent.fsm.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    guard.tick(Event::Shutdown);
                    if let Some(handle) = pending_fetch.take() {
                        handle.abort();
                    }
                    let _ = pending_timer.take();
                    break;
                }
                event = async {
                    if let Some(handle) = pending_fetch.take() {
                        match handle.await {
                            Ok(event) => Some(event),
                            Err(err) => {
                                tracing::warn!(error = %err, "fetch task failed");
                                Some(Event::Response { response: Response::TransientUnavailable, now: std::time::Instant::now() })
                            }
                        }
                    } else {
                        None
                    }
                }, if pending_fetch.is_some() => {
                    if let Some(event) = event {
                        event_queue.push_back(event);
                    }
                }
                _ = async {
                    if let Some(timer) = &mut pending_timer {
                        timer.as_mut().await;
                    }
                }, if pending_timer.is_some() => {
                    pending_timer = None;
                    event_queue.push_back(Event::TimerFired { now: std::time::Instant::now() });
                }
            }
        }
    }
}

async fn fetch_effect(agent: Arc<ConfigAgent>, etag: Option<String>, wait_ms: u64) -> Event {
    match agent.fetch_once(wait_ms, etag).await {
        Ok(event) => event,
        Err(err) => {
            tracing::warn!(error = %err, "fetch failed");
            Event::Response {
                response: Response::TransientUnavailable,
                now: std::time::Instant::now(),
            }
        }
    }
}

impl ConfigAgent {
    async fn fetch_once(&self, wait_ms: u64, etag: Option<String>) -> anyhow::Result<Event> {
        tracing::debug!(
            wait_ms = wait_ms,
            has_etag = etag.is_some(),
            etag = ?etag,
            "fetch_once: preparing HTTP request"
        );
        let url = format!("{}/v1/config?wait_ms={wait_ms}", self.relay_base);
        let mut request = self.client.get(url);
        if let Some(etag) = etag.as_deref() {
            request = request.header("if-none-match", format!("\"{etag}\""));
            tracing::debug!(etag = etag, "fetch_once: setting If-None-Match header");
        } else {
            tracing::debug!("fetch_once: unconditional request (no If-None-Match header)");
        }
        let response = request.send().await;
        let now = std::time::Instant::now();
        let response = match response {
            Ok(response) => response,
            Err(_) => {
                return Ok(Event::Response {
                    response: Response::TransientUnavailable,
                    now,
                });
            }
        };

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
                let bytes = response.bytes().await?;

                Ok(Event::Response {
                    response: Response::NewArtifact {
                        etag: header_etag,
                        version: config_version,
                        size: config_size.or(Some(bytes.len() as u64)),
                        bytes: bytes.to_vec(),
                    },
                    now,
                })
            }
            204 | 304 => Ok(Event::Response {
                response: Response::NoUpdate,
                now,
            }),
            410 => {
                tracing::info!("fetch_once: received 410 Gone from relay, triggering NeedResync");
                Ok(Event::Response {
                    response: Response::NeedResync,
                    now,
                })
            }
            status if (500..=599).contains(&status) => Ok(Event::Response {
                response: Response::TransientUnavailable,
                now,
            }),
            _ => Ok(Event::Response {
                response: Response::TransientUnavailable,
                now,
            }),
        }
    }

    async fn verify_update(
        &self,
        bytes: Vec<u8>,
        expected_etag: String,
        config_version: Option<ConfigVersion>,
        config_size: Option<u64>,
    ) -> anyhow::Result<VerifiedUpdate> {
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
        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
        let current = self.state.load();
        if let Err(err) = validate_env::validate_runtime_env(&validated, Some(&current.config)) {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(self.record_validation_failure(err.into()));
        }
        let _state = RuntimeState::from_config(&validated)
            .map_err(|err| self.record_validation_failure(err))?;

        self.record_validation("ok", "none");
        tracing::info!(
            target: "pavis.config",
            event = "config_validation",
            result = "ok",
            reason = "none",
            etag = expected_etag
        );

        self.record_config_stats(config_version, config_size, "validated update");

        Ok(VerifiedUpdate {
            etag: expected_etag,
            version: config_version,
            size: config_size,
            tmp_path,
        })
    }

    async fn apply_update(
        &self,
        update: VerifiedUpdate,
    ) -> anyhow::Result<(String, Option<ConfigVersion>)> {
        let tmp_path = update.tmp_path.clone();
        let mut state = match pavis_pvs::load(&tmp_path) {
            Ok(config) => {
                let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
                RuntimeState::from_config(&validated)
                    .map_err(|err| self.record_validation_failure(err))?
            }
            Err(err) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(self.record_validation_failure(err.into()));
            }
        };
        state.config_version = update.version;

        tokio::fs::rename(&tmp_path, &self.lkg_path)
            .await
            .map_err(|err| self.record_apply_failure(err.into()))?;

        self.state.store(state);
        self.record_config_stats(update.version, update.size, "applied update");

        if let (Some(handle), Some(version)) = (self.metrics_handle(), update.version) {
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
            let config = pavis_pvs::load(&self.lkg_path)
                .map(|cfg| unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(cfg) })
                .map_err(|err| self.record_validation_failure(err.into()))?;
            callback(&config);
        }

        self.record_apply("ok");
        tracing::info!(
            target: "pavis.config",
            event = "config_apply",
            result = "ok",
            etag = update.etag
        );

        Ok((update.etag, update.version))
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

fn parse_config_version_header(value: &str) -> Option<ConfigVersion> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .and_then(NonZeroU64::new)
        .map(ConfigVersion)
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

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        AccessLogPolicy, ListenerBuilder, ListenerName, LogLevel, Metrics, RuntimeConfigBuilder,
        ServiceName, Telemetry, TracingPolicy,
    };
    use pavis_pvs::PvsError;
    #[test]
    fn test_classify_validation_error() {
        let err = anyhow::anyhow!(PvsError::VersionMismatch {
            file: 2,
            expected: 1
        });
        assert_eq!(classify_validation_error(&err), "version");

        let err = anyhow::anyhow!(PvsError::CorruptArchive("bad".to_string()));
        assert_eq!(classify_validation_error(&err), "parse");

        let err = anyhow::anyhow!("etag/sha256 mismatch: foo");
        assert_eq!(classify_validation_error(&err), "parse");

        let err = anyhow::anyhow!(CoreValidationError::DuplicateUpstream("u".to_string()));
        assert_eq!(classify_validation_error(&err), "semantic");
    }

    #[test]
    fn test_parse_config_version_header() {
        assert_eq!(parse_config_version_header(" 42 ").unwrap().get(), 42);
        assert!(parse_config_version_header("0").is_none());
        assert!(parse_config_version_header("abc").is_none());
    }

    #[test]
    fn test_parse_etag_header() {
        let valid = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(parse_etag_header(valid).unwrap(), valid);

        let quoted = format!("\"{}\"", valid);
        assert_eq!(parse_etag_header(&quoted).unwrap(), valid);

        assert!(parse_etag_header("W/\"weak\"").is_err());
        assert!(parse_etag_header("invalid").is_err());
        assert!(parse_etag_header("sha256:short").is_err());
        assert!(
            parse_etag_header(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg"
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn test_config_agent_bootstrap_invalid_lkg() {
        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("invalid.pvs");
        // Write invalid data
        std::fs::write(&lkg, b"not a pvs").unwrap();

        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = Arc::new(
            ConfigAgent::new(
                "http://localhost".to_string(),
                lkg,
                state,
                Duration::from_secs(1),
            )
            .unwrap(),
        );
        let worker = agent.clone().worker();

        // bootstrap_from_lkg should return error if file is corrupt
        let res = worker.bootstrap_from_lkg().await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_config_agent_bootstrap_env_validation_failure() {
        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("env_fail.pvs");

        // Occupy a port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let l_cfg = ListenerBuilder::new()
            .name(ListenerName("test".to_string()))
            .address(std::net::SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                port,
            ))
            .build()
            .unwrap();
        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            })
            .add_listener(l_cfg)
            .build()
            .unwrap();

        pavis_pvs::write(&lkg, &config).unwrap();

        // Current state does NOT have this port, so validate_runtime_env should fail on preflight bind
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));

        let agent = Arc::new(
            ConfigAgent::new(
                "http://localhost".to_string(),
                lkg,
                state,
                Duration::from_secs(1),
            )
            .unwrap(),
        );
        // Pre-set some etag
        agent.set_last_applied_etag_for_tests(Some("initial".to_string()));

        let worker = agent.clone().worker();

        // bootstrap_from_lkg swallows error and logs warning
        let _ = worker.bootstrap_from_lkg().await;
        // Should NOT have updated last_applied_etag because it failed validation
        assert_eq!(agent.last_applied_etag_for_tests().unwrap(), "initial");

        drop(listener);
    }

    #[tokio::test]
    async fn test_config_agent_bootstrap_success() {
        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("success.pvs");

        let listener = ListenerBuilder::new()
            .name(ListenerName("test".to_string()))
            .address("127.0.0.1:8080".parse().unwrap())
            .build()
            .unwrap();
        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .unwrap();

        let bytes = pavis_pvs::encode(&config).unwrap();
        let etag = checksum_for_bytes(&bytes);
        std::fs::write(&lkg, &bytes).unwrap();

        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = Arc::new(
            ConfigAgent::new(
                "http://localhost".to_string(),
                lkg,
                state,
                Duration::from_secs(1),
            )
            .unwrap(),
        );
        let worker = agent.clone().worker();

        worker.bootstrap_from_lkg().await.unwrap();
        assert_eq!(agent.last_applied_etag_for_tests().unwrap(), etag);
    }

    #[tokio::test]
    async fn test_config_agent_bootstrap_missing_lkg() {
        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("missing.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = Arc::new(
            ConfigAgent::new(
                "http://localhost".to_string(),
                lkg,
                state,
                Duration::from_secs(1),
            )
            .unwrap(),
        );
        let worker = agent.clone().worker();

        // bootstrap_from_lkg should succeed (do nothing) if file missing
        worker.bootstrap_from_lkg().await.unwrap();
        assert!(agent.last_applied_etag_for_tests().is_none());
    }

    #[tokio::test]
    async fn test_config_agent_verify_update_success() {
        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("lkg.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = ConfigAgent::new(
            "http://localhost".to_string(),
            lkg,
            state,
            Duration::from_secs(1),
        )
        .unwrap();

        let listener = ListenerBuilder::new()
            .name(ListenerName("test".to_string()))
            .address("127.0.0.1:8080".parse().unwrap())
            .build()
            .unwrap();
        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .unwrap();

        let bytes = pavis_pvs::encode(&config).unwrap();
        let etag = checksum_for_bytes(&bytes);

        let update = agent
            .verify_update(bytes, etag.clone(), None, None)
            .await
            .unwrap();
        assert_eq!(update.etag, etag);
        assert!(update.tmp_path.exists());
    }

    #[tokio::test]
    async fn test_config_agent_verify_update_etag_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("lkg.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = ConfigAgent::new(
            "http://localhost".to_string(),
            lkg,
            state,
            Duration::from_secs(1),
        )
        .unwrap();

        let res = agent
            .verify_update(
                vec![1, 2, 3],
                "sha256:wrong_length_must_be_64_chars_0123456789abcdef0123456789abcdef01"
                    .to_string(),
                None,
                None,
            )
            .await;
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("etag/sha256 mismatch")
        );
    }

    #[tokio::test]
    async fn test_config_agent_apply_update_success() {
        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("lkg.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = ConfigAgent::new(
            "http://localhost".to_string(),
            lkg.clone(),
            state.clone(),
            Duration::from_secs(1),
        )
        .unwrap();

        let listener = ListenerBuilder::new()
            .name(ListenerName("test".to_string()))
            .address("127.0.0.1:8080".parse().unwrap())
            .build()
            .unwrap();
        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .unwrap();

        let bytes = pavis_pvs::encode(&config).unwrap();
        let etag = checksum_for_bytes(&bytes);
        let tmp_path = tmp_path_for(&lkg);
        write_atomic(&tmp_path, &bytes).await.unwrap();

        let update = VerifiedUpdate {
            etag: etag.clone(),
            version: Some(ConfigVersion(std::num::NonZeroU64::new(1).unwrap())),
            size: Some(bytes.len() as u64),
            tmp_path,
        };

        let (applied_etag, version) = agent.apply_update(update).await.unwrap();
        assert_eq!(applied_etag, etag);
        assert_eq!(version.unwrap().get(), 1);
        assert_eq!(state.load().config_version.unwrap().get(), 1);
        assert!(lkg.exists());
    }

    #[test]
    fn test_config_agent_basic_methods() {
        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("lkg.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = ConfigAgent::new(
            "http://localhost".to_string(),
            lkg,
            state,
            Duration::from_secs(1),
        )
        .unwrap();

        assert_eq!(agent.current_state().state, "Idle");

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        agent.on_update(move |_| {
            called_clone.store(true, Ordering::SeqCst);
        });

        // Test callback triggering manually
        let guard = agent.on_update_callback.lock().unwrap();
        if let Some(cb) = guard.as_ref() {
            let listener = ListenerBuilder::new()
                .name(ListenerName("test".to_string()))
                .address("127.0.0.1:8080".parse().unwrap())
                .build()
                .unwrap();

            let config = RuntimeConfigBuilder::new()
                .telemetry(Telemetry {
                    level: LogLevel::Info,
                    pingora: LogLevel::Info,
                    service_name: ServiceName("test".to_string()),
                    metrics: Metrics::Disabled,
                    access_log: AccessLogPolicy::Disabled,
                    tracing: TracingPolicy::Disabled,
                })
                .add_listener(listener)
                .build()
                .unwrap();
            cb(&config);
        }
        assert!(called.load(Ordering::SeqCst));
    }

    use axum::{Router, extract::State, http::HeaderValue, response::IntoResponse, routing::get};

    #[derive(Clone)]
    struct RelayStub {
        status: Arc<Mutex<u16>>,
        etag: Arc<Mutex<Option<String>>>,
        body: Arc<Mutex<Vec<u8>>>,
    }

    async fn relay_handler(State(stub): State<RelayStub>) -> impl IntoResponse {
        let status = *stub.status.lock().unwrap();
        let etag = stub.etag.lock().unwrap().clone();
        let body = stub.body.lock().unwrap().clone();

        let mut res = if status == 200 {
            body.into_response()
        } else {
            axum::http::StatusCode::from_u16(status)
                .unwrap()
                .into_response()
        };

        if let Some(e) = etag {
            res.headers_mut().insert(
                "etag",
                HeaderValue::from_str(&format!("\"{}\"", e)).unwrap(),
            );
        }
        res
    }

    async fn spawn_stub(stub: RelayStub) -> String {
        let app = Router::new()
            .route("/v1/config", get(relay_handler))
            .with_state(stub);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn test_poll_once_no_update() {
        let stub = RelayStub {
            status: Arc::new(Mutex::new(304)),
            etag: Arc::new(Mutex::new(None)),
            body: Arc::new(Mutex::new(vec![])),
        };
        let url = spawn_stub(stub).await;

        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("lkg.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = ConfigAgent::new(url, lkg, state, Duration::from_secs(1)).unwrap();

        let outcome = agent.poll_once(100).await.unwrap();
        assert!(matches!(outcome, PollOutcome::NoChange));
    }

    #[tokio::test]
    async fn test_poll_once_transient_unavailable() {
        let stub = RelayStub {
            status: Arc::new(Mutex::new(503)),
            etag: Arc::new(Mutex::new(None)),
            body: Arc::new(Mutex::new(vec![])),
        };
        let url = spawn_stub(stub).await;

        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("lkg.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = ConfigAgent::new(url, lkg, state, Duration::from_secs(1)).unwrap();

        let outcome = agent.poll_once(100).await.unwrap();
        assert!(matches!(outcome, PollOutcome::NoChange));
    }

    #[tokio::test]
    async fn test_poll_once_updated() {
        let listener = ListenerBuilder::new()
            .name(ListenerName("test".to_string()))
            .address("127.0.0.1:8080".parse().unwrap())
            .build()
            .unwrap();
        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .unwrap();

        let bytes = pavis_pvs::encode(&config).unwrap();
        let etag = checksum_for_bytes(&bytes);

        let stub = RelayStub {
            status: Arc::new(Mutex::new(200)),
            etag: Arc::new(Mutex::new(Some(etag.clone()))),
            body: Arc::new(Mutex::new(bytes)),
        };
        let url = spawn_stub(stub).await;

        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("lkg.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = ConfigAgent::new(url, lkg, state, Duration::from_secs(1)).unwrap();

        let outcome = agent.poll_once(100).await.unwrap();
        assert!(matches!(outcome, PollOutcome::Updated));
        assert_eq!(agent.last_applied_etag_for_tests().unwrap(), etag);
    }

    #[tokio::test]
    async fn test_poll_once_verify_failure() {
        let stub = RelayStub {
            status: Arc::new(Mutex::new(200)),
            // Corrupt ETag or body will cause verify failure
            etag: Arc::new(Mutex::new(Some(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_string(),
            ))),
            body: Arc::new(Mutex::new(b"invalid".to_vec())),
        };
        let url = spawn_stub(stub).await;

        let dir = tempfile::tempdir().unwrap();
        let lkg = dir.path().join("lkg.pvs");
        let state = Arc::new(RuntimeStateHandle::new(RuntimeState::default()));
        let agent = ConfigAgent::new(url, lkg, state, Duration::from_secs(1)).unwrap();

        let outcome = agent.poll_once(100).await.unwrap();
        assert!(matches!(outcome, PollOutcome::Rejected));
    }
}
