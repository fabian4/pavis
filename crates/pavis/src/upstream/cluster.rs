use super::load_balance;
use pavis_core::{Endpoint, Upstream};
use std::sync::atomic::AtomicUsize;

#[repr(align(64))]
#[derive(Debug)]
pub struct AlignedCounter(pub AtomicUsize);

#[derive(Debug)]
pub struct Cluster {
    pub config: Upstream,
    // Co-located state
    pub rr_counter: AlignedCounter,
    pub total_weight: u32,
}

impl Cluster {
    pub fn new(config: Upstream) -> Self {
        let total_weight = config.endpoints.iter().map(|e| e.weight).sum();
        Self {
            config,
            rr_counter: AlignedCounter(AtomicUsize::new(0)),
            total_weight,
        }
    }

    pub fn select_endpoint(&self) -> Option<&Endpoint> {
        if self.config.endpoints.is_empty() {
            return None;
        }
        let idx = load_balance::select_index(
            self.config.load_balancer,
            &self.config.endpoints,
            &self.rr_counter.0,
            self.total_weight,
        );
        self.config.endpoints.get(idx)
    }
}
