use crate::config::PersistenceOptions;
use axum::body::Bytes;
use pavis_core::RuntimeConfig;
use pavis_pvs::PvsHeaderView;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use tokio::sync::{Notify, RwLock, watch};
use tracing::{debug, warn};

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub(crate) enum RelayError {
    #[error("single source authority violation: {0}")]
    SingleSource(String),
    #[error("version monotonicity violation: current={current}, proposed={proposed}")]
    VersionMonotonicity { current: u64, proposed: u64 },
    #[error("cache error: {0}")]
    Cache(String),
    #[error("storage error: {0}")]
    Storage(#[from] std::io::Error),
    #[error("policy enforcement error: {0}")]
    Policy(String),
    #[error("http server error: {0}")]
    Http(String),
    #[error("config error: {0}")]
    Config(String),
}

pub(crate) fn execute_plan(current_version: u64, proposed_version: u64) -> Result<(), RelayError> {
    if proposed_version <= current_version {
        return Err(RelayError::VersionMonotonicity {
            current: current_version,
            proposed: proposed_version,
        });
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct RelaySnapshot {
    version: u64,
    artifact: RelayArtifact,
    updated_at: SystemTime,
}

#[derive(Debug, Clone)]
pub(crate) struct RelaySnapshotView {
    pub version: u64,
    pub pvs_bytes: Bytes,
    pub meta: RelayMeta,
    pub updated_at: SystemTime,
}

#[derive(Debug, Clone)]
pub(crate) struct RelayArtifactView {
    pub bytes: Bytes,
    pub meta: RelayMeta,
}

#[derive(Debug, Clone)]
struct RelayArtifact {
    bytes: Bytes,
    meta: RelayMeta,
}

#[derive(Debug, Clone)]
pub(crate) struct RelayMeta {
    pub checksum: String,
    pub algorithm: String,
    #[allow(dead_code)]
    pub schema_version: u32,
}

impl RelayMeta {
    pub fn empty() -> Self {
        Self {
            checksum: String::new(),
            algorithm: String::new(),
            schema_version: 0,
        }
    }

    pub fn from_header(header: &PvsHeaderView) -> Self {
        Self {
            checksum: header.checksum_hex(),
            algorithm: header.algorithm_label(),
            schema_version: header.version(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RelayState {
    inner: Arc<RwLock<RelaySnapshot>>,
    history: Arc<RwLock<HashMap<u64, RelayArtifact>>>,
    notify: Arc<Notify>,
    options: RelayOptions,
    metrics: Arc<RelayMetrics>,
    last_error: Arc<RwLock<Option<String>>>,
    started_at: SystemTime,
    persistence: Option<PersistenceHandle>,
}

#[derive(Clone, Debug)]
pub(crate) struct RelayOptions {
    pub version_header: axum::http::HeaderName,
    pub checksum_header: axum::http::HeaderName,
    pub checksum_alg_header: axum::http::HeaderName,
    pub long_poll_enabled: bool,
    pub identity_name: String,
    pub lkg_path: Option<PathBuf>,
    pub persistence: PersistenceOptions,
    pub max_pvs_bytes: u64,
}

impl Default for RelayOptions {
    fn default() -> Self {
        Self {
            version_header: axum::http::HeaderName::from_static(pavis_pvs::PAVIS_VERSION_HEADER),
            checksum_header: axum::http::HeaderName::from_static(pavis_pvs::PAVIS_CHECKSUM_HEADER),
            checksum_alg_header: axum::http::HeaderName::from_static(
                pavis_pvs::PAVIS_CHECKSUM_ALG_HEADER,
            ),
            long_poll_enabled: true,
            identity_name: String::new(),
            lkg_path: None,
            persistence: PersistenceOptions::default(),
            max_pvs_bytes: 0,
        }
    }
}

#[derive(Clone)]
struct PersistenceHandle {
    tx: watch::Sender<Bytes>,
    shutdown: watch::Sender<bool>,
}

impl Drop for PersistenceHandle {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

impl RelayState {
    #[allow(dead_code)]
    pub(crate) fn new(version: u64, pvs_bytes: Bytes) -> Result<Self, RelayError> {
        Self::new_with_options(version, pvs_bytes, RelayOptions::default())
    }

    pub(crate) fn new_with_options(
        version: u64,
        pvs_bytes: Bytes,
        options: RelayOptions,
    ) -> Result<Self, RelayError> {
        let last_error = Arc::new(RwLock::new(None));
        let meta = if pvs_bytes.is_empty() {
            RelayMeta::empty()
        } else {
            let header = pavis_pvs::inspect(&pvs_bytes)
                .map_err(|err| RelayError::Config(err.to_string()))?;
            RelayMeta::from_header(&header)
        };
        let artifact = RelayArtifact {
            bytes: pvs_bytes.clone(),
            meta: meta.clone(),
        };
        let mut history = HashMap::new();
        history.insert(version, artifact.clone());
        let persistence =
            if let (Some(path), true) = (options.lkg_path.clone(), options.persistence.enabled) {
                if tokio::runtime::Handle::try_current().is_ok() {
                    Some(start_persistence(
                        path,
                        options.persistence,
                        last_error.clone(),
                    ))
                } else {
                    warn!("Persistence disabled (no Tokio runtime available)");
                    None
                }
            } else {
                None
            };
        Ok(Self {
            inner: Arc::new(RwLock::new(RelaySnapshot {
                version,
                artifact,
                updated_at: SystemTime::now(),
            })),
            history: Arc::new(RwLock::new(history)),
            notify: Arc::new(Notify::new()),
            options,
            metrics: Arc::new(RelayMetrics::default()),
            last_error,
            started_at: SystemTime::now(),
            persistence,
        })
    }

    pub(crate) async fn version(&self) -> u64 {
        self.inner.read().await.version
    }

    pub(crate) async fn snapshot(&self) -> RelaySnapshotView {
        let snapshot = self.inner.read().await;
        RelaySnapshotView {
            version: snapshot.version,
            pvs_bytes: snapshot.artifact.bytes.clone(),
            meta: snapshot.artifact.meta.clone(),
            updated_at: snapshot.updated_at,
        }
    }

    pub(crate) async fn publish_config(&self, config: &RuntimeConfig) -> Result<u64, RelayError> {
        let bytes = pavis_pvs::encode(config).map_err(|e| RelayError::Config(e.to_string()))?;
        let version = self.publish_auto(bytes.into()).await?;
        debug!("Published config from struct: version={}", version);
        Ok(version)
    }

    pub(crate) async fn publish_auto(&self, bytes: Bytes) -> Result<u64, RelayError> {
        self.enforce_limits(bytes.len())?;
        let header =
            pavis_pvs::inspect(&bytes).map_err(|err| RelayError::Config(err.to_string()))?;
        let meta = RelayMeta::from_header(&header);

        let mut inner = self.inner.write().await;
        let proposed_version = inner.version + 1;

        debug!(
            "Publishing auto-increment version: {} -> {}, checksum={}",
            inner.version, proposed_version, meta.checksum
        );

        inner.version = proposed_version;
        inner.artifact = RelayArtifact {
            bytes: bytes.clone(),
            meta: meta.clone(),
        };
        inner.updated_at = SystemTime::now();
        drop(inner);

        let bytes_for_persist = bytes.clone();
        let mut history = self.history.write().await;
        history.insert(proposed_version, RelayArtifact { bytes, meta });
        drop(history);
        self.notify.notify_waiters();
        self.metrics.inc_publish_ok();

        if let Some(persistence) = self.persistence.as_ref() {
            let _ = persistence.tx.send_replace(bytes_for_persist);
        }

        Ok(proposed_version)
    }

    pub(crate) async fn publish(
        &self,
        proposed_version: u64,
        bytes: Bytes,
        meta: RelayMeta,
    ) -> Result<(), RelayError> {
        self.enforce_limits(bytes.len())?;
        let mut inner = self.inner.write().await;
        execute_plan(inner.version, proposed_version)?;

        inner.version = proposed_version;
        inner.artifact = RelayArtifact {
            bytes: bytes.clone(),
            meta: meta.clone(),
        };
        inner.updated_at = SystemTime::now();
        drop(inner);

        let bytes_for_persist = bytes.clone();
        let mut history = self.history.write().await;
        history.insert(proposed_version, RelayArtifact { bytes, meta });
        drop(history);
        self.notify.notify_waiters();

        if let Some(persistence) = self.persistence.as_ref() {
            let _ = persistence.tx.send_replace(bytes_for_persist);
        }

        Ok(())
    }

    pub(crate) async fn artifact(&self, version: u64) -> Option<RelayArtifactView> {
        let history = self.history.read().await;
        history.get(&version).map(|artifact| RelayArtifactView {
            bytes: artifact.bytes.clone(),
            meta: artifact.meta.clone(),
        })
    }

    pub(crate) fn notifier(&self) -> &Notify {
        &self.notify
    }

    pub(crate) fn options(&self) -> &RelayOptions {
        &self.options
    }

    pub(crate) fn metrics(&self) -> &RelayMetrics {
        &self.metrics
    }

    pub(crate) async fn set_last_error(&self, value: Option<String>) {
        let mut guard = self.last_error.write().await;
        *guard = value;
    }

    #[allow(dead_code)]
    pub(crate) async fn last_error(&self) -> Option<String> {
        self.last_error.read().await.clone()
    }

    pub(crate) fn started_at(&self) -> SystemTime {
        self.started_at
    }

    fn enforce_limits(&self, size: usize) -> Result<(), RelayError> {
        let limit = self.options.max_pvs_bytes;
        if limit > 0 && (size as u64) > limit {
            return Err(RelayError::Policy(format!(
                "pvs size {} exceeds max_pvs_bytes {}",
                size, limit
            )));
        }
        Ok(())
    }
}

fn start_persistence(
    path: PathBuf,
    options: PersistenceOptions,
    last_error: Arc<RwLock<Option<String>>>,
) -> PersistenceHandle {
    let (tx, mut rx) = watch::channel(Bytes::new());
    let (shutdown, mut shutdown_rx) = watch::channel(false);

    tokio::spawn(async move {
        let mut pending: Option<Bytes> = None;
        let mut interval = tokio::time::interval(options.flush_interval);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if rx.has_changed().unwrap_or(false) {
                        let latest = rx.borrow_and_update().clone();
                        pending = Some(latest);
                    }
                }
                _ = rx.changed() => {
                    let latest = rx.borrow_and_update().clone();
                    pending = Some(latest);
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        let bytes = pending.take().or_else(|| {
                            let current = rx.borrow().clone();
                            if current.is_empty() {
                                None
                            } else {
                                Some(current)
                            }
                        });

                        if let Some(bytes) = bytes {
                            let _ = persist_with_retry(&path, bytes, options)
                                .await
                                .inspect_err(|err| warn!("Persist to disk failed during shutdown: {}", err));
                        }
                        break;
                    }
                }
            }

            let Some(bytes) = pending.clone() else {
                continue;
            };

            match persist_with_retry(&path, bytes.clone(), options).await {
                Ok(()) => {
                    let mut guard = last_error.write().await;
                    *guard = None;
                    pending = None;
                }
                Err(err) => {
                    warn!("Persist to disk failed: {}", err);
                    let mut guard = last_error.write().await;
                    *guard = Some(err.to_string());
                }
            }
        }
    });

    PersistenceHandle { tx, shutdown }
}

async fn persist_with_retry(
    path: &std::path::Path,
    bytes: Bytes,
    options: PersistenceOptions,
) -> Result<(), RelayError> {
    let mut attempt = 0;
    let mut delay = options.retry_backoff;
    let tmp_path = path.with_extension("tmp");

    loop {
        attempt += 1;
        let write_result = async {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::File::create(&tmp_path).await?;
            file.write_all(&bytes).await?;
            file.sync_all().await?;
            drop(file);
            tokio::fs::rename(&tmp_path, path).await?;
            Ok::<(), std::io::Error>(())
        }
        .await;

        match write_result {
            Ok(()) => return Ok(()),
            Err(err) if attempt <= options.retry_max => {
                warn!(
                    "Persist attempt {} of {} failed: {}",
                    attempt, options.retry_max, err
                );
                tokio::time::sleep(delay).await;
                delay = std::cmp::min(delay.saturating_mul(2), options.retry_backoff_max);
            }
            Err(err) => return Err(RelayError::Storage(err)),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RelayMetrics {
    publish_ok: AtomicU64,
    publish_fail: AtomicU64,
    long_poll_wait: AtomicU64,
}

impl RelayMetrics {
    pub(crate) fn inc_publish_ok(&self) {
        self.publish_ok.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn inc_publish_fail(&self) {
        self.publish_fail.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn inc_long_poll_wait(&self) {
        self.long_poll_wait.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn publish_ok(&self) -> u64 {
        self.publish_ok.load(Ordering::Relaxed)
    }

    pub(crate) fn publish_fail(&self) -> u64 {
        self.publish_fail.load(Ordering::Relaxed)
    }

    pub(crate) fn long_poll_wait(&self) -> u64 {
        self.long_poll_wait.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::{RelayMeta, RelayOptions, RelayState, execute_plan};
    use axum::body::Bytes;

    #[test]
    fn execute_plan_rejects_non_monotonic_versions() {
        let err = execute_plan(5, 5).expect_err("non-monotonic");
        match err {
            super::RelayError::VersionMonotonicity { current, proposed } => {
                assert_eq!(current, 5);
                assert_eq!(proposed, 5);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn state_tracks_version_and_snapshot() {
        let state = RelayState::new(3, Bytes::new()).expect("state");
        assert_eq!(state.version().await, 3);
        let snapshot = state.snapshot().await;
        assert_eq!(snapshot.version, 3);
        assert!(snapshot.meta.checksum.is_empty());

        let meta = RelayMeta {
            checksum: "sum".to_string(),
            algorithm: "alg".to_string(),
            schema_version: 0,
        };
        state
            .publish(4, Bytes::from_static(b"bytes"), meta.clone())
            .await
            .expect("publish");
        let snapshot = state.snapshot().await;
        assert_eq!(snapshot.version, 4);
        assert_eq!(snapshot.meta.checksum, "sum");
    }

    #[tokio::test]
    async fn state_publish_auto_increments_version() {
        // Use a valid PVS for publish_auto as it inspects the header
        let config = pavis_core::RuntimeConfig {
            server: pavis_core::ServerConfig {
                listen_addr: "127.0.0.1:8080".parse().unwrap(),
                worker_threads: None,
                tls: None,
            },
            telemetry: pavis_core::TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: pavis_core::AccessLogConfig::Disabled,
                tracing: None,
            },
            upstreams: vec![],
            routes: vec![],
        };
        let pvs_bytes = pavis_pvs::encode(&config).expect("encode");

        let state = RelayState::new(10, Bytes::new()).expect("state");
        assert_eq!(state.version().await, 10);

        let v11 = state.publish_auto(pvs_bytes.into()).await.expect("publish");
        assert_eq!(v11, 11);
        assert_eq!(state.version().await, 11);
    }

    #[tokio::test]
    async fn state_tracks_last_error() {
        let state = RelayState::new(0, Bytes::new()).expect("state");
        assert!(state.last_error().await.is_none());
        state.set_last_error(Some("test error".to_string())).await;
        assert_eq!(state.last_error().await, Some("test error".to_string()));
    }

    #[test]
    fn relay_meta_empty_has_defaults() {
        let meta = RelayMeta::empty();
        assert!(meta.checksum.is_empty());
        assert!(meta.algorithm.is_empty());
        assert_eq!(meta.schema_version, 0);
    }

    #[tokio::test]
    async fn state_returns_none_for_missing_artifact() {
        let state = RelayState::new(0, Bytes::new()).expect("state");
        assert!(state.artifact(999).await.is_none());
    }

    #[tokio::test]
    async fn state_publish_enforces_monotonicity() {
        let state = RelayState::new(10, Bytes::new()).expect("state");
        let meta = RelayMeta::empty();

        // Same version
        let err = state
            .publish(10, Bytes::from_static(b"data"), meta.clone())
            .await
            .expect_err("same version");
        assert!(matches!(
            err,
            super::RelayError::VersionMonotonicity {
                current: 10,
                proposed: 10
            }
        ));

        // Older version
        let err = state
            .publish(5, Bytes::from_static(b"data"), meta.clone())
            .await
            .expect_err("older version");
        assert!(matches!(
            err,
            super::RelayError::VersionMonotonicity {
                current: 10,
                proposed: 5
            }
        ));

        // Newer version (ok)
        state
            .publish(11, Bytes::from_static(b"data"), meta)
            .await
            .expect("newer version");
        assert_eq!(state.version().await, 11);
    }

    #[tokio::test]
    async fn persistence_updates_last_error_on_failure() {
        let dir = std::env::temp_dir().join("relay_persist_fail");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Use a directory as the LKG path to force IsADirectory error on write
        let lkg = dir.join("config.pvs");
        std::fs::create_dir(&lkg).unwrap();

        let mut options = RelayOptions::default();
        options.persistence.enabled = true;
        options.persistence.flush_interval = std::time::Duration::from_millis(10);
        options.persistence.retry_max = 1; // fast fail
        options.persistence.retry_backoff = std::time::Duration::from_millis(1);
        options.lkg_path = Some(lkg.clone());

        // Use valid PVS bytes
        let config = pavis_core::RuntimeConfig {
            server: pavis_core::ServerConfig {
                listen_addr: "127.0.0.1:8080".parse().unwrap(),
                worker_threads: None,
                tls: None,
            },
            telemetry: pavis_core::TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: pavis_core::AccessLogConfig::Disabled,
                tracing: None,
            },
            upstreams: vec![],
            routes: vec![],
        };
        let pvs_bytes = pavis_pvs::encode(&config).expect("encode");

        let state = RelayState::new_with_options(0, pvs_bytes.into(), options).expect("state");

        // Trigger persistence with valid bytes
        let update_bytes = pavis_pvs::encode(&config).expect("encode");
        state
            .publish_auto(update_bytes.into())
            .await
            .expect("publish");

        // Wait for persistence loop to fail
        let mut attempts = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if state.last_error().await.is_some() {
                break;
            }
            attempts += 1;
            if attempts > 20 {
                panic!("timed out waiting for persistence error");
            }
        }

        let err = state.last_error().await.unwrap();
        // Error will be "Is a directory" (Os { code: 21 })
        assert!(err.contains("Is a directory") || err.contains("Os { code: 21"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
