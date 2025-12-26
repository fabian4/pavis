use super::load_balance;
use crate::config::{Endpoint, Upstream};
use std::sync::atomic::AtomicUsize;

#[repr(align(64))]
#[derive(Debug)]
pub struct AlignedCounter(pub AtomicUsize);

#[derive(Debug)]
pub struct Cluster {
    pub config: Upstream,
    // Co-located state
    pub rr_counter: AlignedCounter,
}

impl Cluster {
    pub fn new(config: Upstream) -> Self {
        Self {
            config,
            rr_counter: AlignedCounter(AtomicUsize::new(0)),
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
        );
        self.config.endpoints.get(idx)
    }
}
