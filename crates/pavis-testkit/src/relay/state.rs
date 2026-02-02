use crate::common::cli::RelayArgs;
use crate::relay::types::ArtifactMeta;
use axum::extract::FromRef;
use bytes::Bytes;
use pavis_pvs::compute_checksum;
use serde::Serialize;
use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    mode: Option<MockMode>,
    script_counter: Arc<AtomicUsize>,
}

struct InnerState {
    data: Option<Bytes>,
    meta: Option<ArtifactMeta>,
    requests: Vec<RequestRecord>,
    resync_completed: bool,
    gone_triggered: bool,
}

#[derive(Clone, Serialize)]
pub struct RequestRecord {
    pub wait_ms: Option<u64>,
    pub if_none_match: Option<String>,
}

impl RelayState {
    pub fn new() -> Self {
        Self::new_with_mode(None)
    }

    pub fn new_with_mode(mode: Option<MockMode>) -> Self {
        let (tx, _) = watch::channel(0);
        Self {
            inner: Arc::new(RwLock::new(InnerState {
                data: None,
                meta: None,
                requests: Vec::new(),
                resync_completed: false,
                gone_triggered: false,
            })),
            notifier: tx,
            mode,
            script_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn publish(&self, data: Bytes) -> ArtifactMeta {
        let mut inner = self.inner.write().await;
        let next_rev = inner.meta.as_ref().map(|m| m.rev).unwrap_or(0) + 1;

        let checksum = checksum_for_bytes(&data);
        let etag = checksum.clone();

        let meta = ArtifactMeta {
            rev: next_rev,
            etag,
            size: data.len(),
            checksum,
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

    pub async fn record_request(&self, wait_ms: Option<u64>, if_none_match: Option<String>) {
        const MAX_REQUEST_LOG: usize = 1024;
        let mut inner = self.inner.write().await;
        if inner.requests.len() >= MAX_REQUEST_LOG {
            inner.requests.remove(0);
        }
        inner.requests.push(RequestRecord {
            wait_ms,
            if_none_match,
        });
    }

    pub async fn get_requests(&self) -> Vec<RequestRecord> {
        let inner = self.inner.read().await;
        inner.requests.clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.notifier.subscribe()
    }

    pub fn mock_mode(&self) -> Option<MockMode> {
        self.mode
    }

    pub fn next_script_attempt(&self) -> usize {
        self.script_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn check_and_mark_resync(&self, is_unconditional: bool) -> bool {
        let mut inner = self.inner.write().await;
        if is_unconditional && !inner.resync_completed {
            inner.resync_completed = true;
            false 
        } else {
            inner.resync_completed
        }
    }

    pub async fn check_and_mark_gone(&self) -> bool {
        let mut inner = self.inner.write().await;
        if inner.gone_triggered {
            true
        } else {
            inner.gone_triggered = true;
            false
        }
    }
}

impl Default for RelayState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub enum MockMode {
    ResyncOnce,
    CorruptOnce,
    CorruptRepeat,
}

impl MockMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "resync-once" => Some(Self::ResyncOnce),
            "corrupt-once" => Some(Self::CorruptOnce),
            "corrupt-repeat" => Some(Self::CorruptRepeat),
            _ => None,
        }
    }
}

fn checksum_for_bytes(bytes: &[u8]) -> String {
    let digest = pavis_pvs::compute_checksum(bytes);
    let mut out = String::with_capacity(digest.len() * 2 + "sha256:".len());
    out.push_str("sha256:");
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
