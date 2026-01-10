#[cfg(feature = "metrics")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "metrics")]
#[derive(Default)]
pub struct Metrics {
    requests_total: AtomicU64,
}

#[cfg(feature = "metrics")]
impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_request(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn requests_total(&self) -> u64 {
        self.requests_total.load(Ordering::Relaxed)
    }
}

#[cfg(not(feature = "metrics"))]
#[derive(Clone, Copy, Default)]
pub struct Metrics;

#[cfg(not(feature = "metrics"))]
impl Metrics {
    pub fn new() -> Self {
        Self
    }

    pub fn record_request(&self) {}

    pub fn requests_total(&self) -> u64 {
        0
    }
}
