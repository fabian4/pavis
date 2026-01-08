use axum::body::Bytes;
use pavis_core::ValidatedRuntimeConfig;
use pavis_pvs::PvsHeaderView;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use tokio::sync::{Notify, RwLock};
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
    pub generated_at: SystemTime,
}

#[derive(Debug, Clone)]
struct RelayArtifact {
    bytes: Bytes,
    meta: RelayMeta,
    generated_at: SystemTime,
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
}

#[derive(Clone, Debug)]
pub(crate) struct RelayOptions {
    pub version_header: axum::http::HeaderName,
    pub checksum_header: axum::http::HeaderName,
    pub checksum_alg_header: axum::http::HeaderName,
    pub generated_at_header: axum::http::HeaderName,
    pub long_poll_enabled: bool,
    pub identity_name: String,
    pub lkg_path: Option<PathBuf>,
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
            generated_at_header: axum::http::HeaderName::from_static(
                pavis_pvs::PAVIS_GENERATED_AT_HEADER,
            ),
            long_poll_enabled: true,
            identity_name: String::new(),
            lkg_path: None,
            max_pvs_bytes: 0,
        }
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
        let now = SystemTime::now();
        let artifact = RelayArtifact {
            bytes: pvs_bytes.clone(),
            meta: meta.clone(),
            generated_at: now,
        };
        let mut history = HashMap::new();
        history.insert(version, artifact.clone());

        Ok(Self {
            inner: Arc::new(RwLock::new(RelaySnapshot {
                version,
                artifact,
                updated_at: now,
            })),
            history: Arc::new(RwLock::new(history)),
            notify: Arc::new(Notify::new()),
            options,
            metrics: Arc::new(RelayMetrics::default()),
            last_error,
            started_at: SystemTime::now(),
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

    pub(crate) async fn publish_config(
        &self,
        config: &ValidatedRuntimeConfig,
    ) -> Result<u64, RelayError> {
        let bytes =
            pavis_pvs::encode(config.as_ref()).map_err(|e| RelayError::Config(e.to_string()))?;
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
        let now = SystemTime::now();

        debug!(
            "Publishing auto-increment version: {} -> {}, checksum={}",
            inner.version, proposed_version, meta.checksum
        );

        if let Some(path) = self.options.lkg_path.as_ref() {
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            if let Err(err) = tokio::fs::write(path, &bytes).await {
                warn!("Failed to write LKG during publish_auto: {}", err);
                return Err(RelayError::Storage(err));
            }
        }

        inner.version = proposed_version;
        let artifact = RelayArtifact {
            bytes: bytes.clone(),
            meta: meta.clone(),
            generated_at: now,
        };
        inner.artifact = artifact.clone();
        inner.updated_at = now;
        drop(inner);

        let mut history = self.history.write().await;
        history.insert(proposed_version, artifact);
        drop(history);
        self.notify.notify_waiters();
        self.metrics.inc_publish_ok();

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

        if let Some(path) = self.options.lkg_path.as_ref() {
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            if let Err(err) = tokio::fs::write(path, &bytes).await {
                warn!("Failed to write LKG during publish: {}", err);
                return Err(RelayError::Storage(err));
            }
        }

        let now = SystemTime::now();
        inner.version = proposed_version;
        let artifact = RelayArtifact {
            bytes: bytes.clone(),
            meta: meta.clone(),
            generated_at: now,
        };
        inner.artifact = artifact.clone();
        inner.updated_at = now;
        drop(inner);

        let mut history = self.history.write().await;
        history.insert(proposed_version, artifact);
        drop(history);
        self.notify.notify_waiters();

        Ok(())
    }

    pub(crate) async fn artifact(&self, version: u64) -> Option<RelayArtifactView> {
        let history = self.history.read().await;
        history.get(&version).map(|artifact| RelayArtifactView {
            bytes: artifact.bytes.clone(),
            meta: artifact.meta.clone(),
            generated_at: artifact.generated_at,
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
mod tests;
