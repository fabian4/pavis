use axum::body::Bytes;
use pavis_pvs::PvsHeaderView;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use tokio::sync::{Notify, RwLock};

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
}

#[derive(Clone, Debug)]
pub(crate) struct RelayOptions {
    pub version_header: axum::http::HeaderName,
    pub checksum_header: axum::http::HeaderName,
    pub checksum_alg_header: axum::http::HeaderName,
    pub long_poll_enabled: bool,
    pub identity_name: String,
    pub lkg_path: Option<PathBuf>,
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
            last_error: Arc::new(RwLock::new(None)),
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

    pub(crate) async fn publish(
        &self,
        proposed_version: u64,
        bytes: Bytes,
        meta: RelayMeta,
    ) -> Result<(), RelayError> {
        let mut inner = self.inner.write().await;
        execute_plan(inner.version, proposed_version)?;

        inner.version = proposed_version;
        inner.artifact = RelayArtifact {
            bytes: bytes.clone(),
            meta: meta.clone(),
        };
        inner.updated_at = SystemTime::now();
        drop(inner);

        let mut history = self.history.write().await;
        history.insert(proposed_version, RelayArtifact { bytes, meta });
        drop(history);
        self.notify.notify_waiters();

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
    use super::{RelayMeta, RelayState, execute_plan};
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
}
