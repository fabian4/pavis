use axum::body::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
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

#[derive(Debug)]
struct RelaySnapshot {
    version: u64,
    pvs_bytes: Bytes,
}

#[derive(Clone)]
pub struct RelayState {
    inner: Arc<RwLock<RelaySnapshot>>,
    history: Arc<RwLock<HashMap<u64, Bytes>>>,
    notify: Arc<Notify>,
}

impl RelayState {
    pub fn new(version: u64, pvs_bytes: Bytes) -> Result<Self, RelayError> {
        let mut history = HashMap::new();
        history.insert(version, pvs_bytes.clone());
        Ok(Self {
            inner: Arc::new(RwLock::new(RelaySnapshot { version, pvs_bytes })),
            history: Arc::new(RwLock::new(history)),
            notify: Arc::new(Notify::new()),
        })
    }

    pub async fn version(&self) -> u64 {
        self.inner.read().await.version
    }

    pub async fn snapshot(&self) -> (u64, Bytes) {
        let snapshot = self.inner.read().await;
        (snapshot.version, snapshot.pvs_bytes.clone())
    }

    pub async fn publish(&self, proposed_version: u64, body: Bytes) -> Result<(), RelayError> {
        let mut inner = self.inner.write().await;
        execute_plan(inner.version, proposed_version)?;

        inner.version = proposed_version;
        inner.pvs_bytes = body.clone();
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
}
