use axum::body::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{Notify, RwLock};

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
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

pub fn execute_plan(current_version: u64, proposed_version: u64) -> Result<(), RelayError> {
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
    pvs_bytes: Bytes,
    updated_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct RelaySnapshotView {
    pub version: u64,
    pub pvs_bytes: Bytes,
    pub updated_at: SystemTime,
}

#[derive(Clone)]
pub struct RelayState {
    inner: Arc<RwLock<RelaySnapshot>>,
    history: Arc<RwLock<HashMap<u64, Bytes>>>,
    notify: Arc<Notify>,
    options: RelayOptions,
}

#[derive(Clone, Debug)]
pub struct RelayOptions {
    pub version_header: axum::http::HeaderName,
    pub checksum_header: axum::http::HeaderName,
    pub checksum_alg_header: axum::http::HeaderName,
    pub long_poll_enabled: bool,
    pub identity_name: String,
}

impl Default for RelayOptions {
    fn default() -> Self {
        Self {
            version_header: axum::http::HeaderName::from_static("x-pavis-version"),
            checksum_header: axum::http::HeaderName::from_static("x-pavis-checksum"),
            checksum_alg_header: axum::http::HeaderName::from_static("x-pavis-checksum-alg"),
            long_poll_enabled: true,
            identity_name: String::new(),
        }
    }
}

impl RelayState {
    pub fn new(version: u64, pvs_bytes: Bytes) -> Result<Self, RelayError> {
        Self::new_with_options(version, pvs_bytes, RelayOptions::default())
    }

    pub fn new_with_options(
        version: u64,
        pvs_bytes: Bytes,
        options: RelayOptions,
    ) -> Result<Self, RelayError> {
        let mut history = HashMap::new();
        history.insert(version, pvs_bytes.clone());
        Ok(Self {
            inner: Arc::new(RwLock::new(RelaySnapshot {
                version,
                pvs_bytes,
                updated_at: SystemTime::now(),
            })),
            history: Arc::new(RwLock::new(history)),
            notify: Arc::new(Notify::new()),
            options,
        })
    }

    pub async fn version(&self) -> u64 {
        self.inner.read().await.version
    }

    pub async fn snapshot(&self) -> RelaySnapshotView {
        let snapshot = self.inner.read().await;
        RelaySnapshotView {
            version: snapshot.version,
            pvs_bytes: snapshot.pvs_bytes.clone(),
            updated_at: snapshot.updated_at,
        }
    }

    pub async fn publish(&self, proposed_version: u64, body: Bytes) -> Result<(), RelayError> {
        let mut inner = self.inner.write().await;
        execute_plan(inner.version, proposed_version)?;

        inner.version = proposed_version;
        inner.pvs_bytes = body.clone();
        inner.updated_at = SystemTime::now();
        drop(inner);

        let mut history = self.history.write().await;
        history.insert(proposed_version, body);
        drop(history);
        self.notify.notify_waiters();

        Ok(())
    }

    pub async fn artifact(&self, version: u64) -> Option<Bytes> {
        let history = self.history.read().await;
        history.get(&version).cloned()
    }

    pub fn notifier(&self) -> &Notify {
        &self.notify
    }

    pub fn options(&self) -> &RelayOptions {
        &self.options
    }
}
