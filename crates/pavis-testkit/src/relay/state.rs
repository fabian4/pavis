use crate::common::cli::RelayArgs;
use crate::relay::types::ArtifactMeta;
use axum::extract::FromRef;
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::{RwLock, watch};

#[derive(Clone, FromRef)]
pub struct AppState {
    pub state: RelayState,
    pub args: RelayArgs,
}

#[derive(Clone)]
pub struct RelayState {
    inner: Arc<RwLock<InnerState>>,
    notifier: watch::Sender<u64>, // sends 'rev'
}

struct InnerState {
    data: Option<Bytes>,
    meta: Option<ArtifactMeta>,
}

impl RelayState {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(0);
        Self {
            inner: Arc::new(RwLock::new(InnerState {
                data: None,
                meta: None,
            })),
            notifier: tx,
        }
    }
}

impl Default for RelayState {
    fn default() -> Self {
        Self::new()
    }
}

impl RelayState {
    pub async fn publish(&self, data: Bytes) -> ArtifactMeta {
        let mut inner = self.inner.write().await;
        let next_rev = inner.meta.as_ref().map(|m| m.rev).unwrap_or(0) + 1;
        let etag = format!("rev-{}", next_rev);
        let meta = ArtifactMeta {
            rev: next_rev,
            etag,
            size: data.len(),
        };

        inner.data = Some(data);
        inner.meta = Some(meta.clone());

        let _ = self.notifier.send(next_rev);
        meta
    }

    pub async fn get_current(&self) -> Option<(ArtifactMeta, Bytes)> {
        let inner = self.inner.read().await;
        match (&inner.meta, &inner.data) {
            (Some(m), Some(d)) => Some((m.clone(), d.clone())),
            _ => None,
        }
    }

    pub async fn get_meta(&self) -> Option<ArtifactMeta> {
        let inner = self.inner.read().await;
        inner.meta.clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notifier.subscribe()
    }
}
