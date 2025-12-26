//! Upstream module: Backend cluster management and load balancing.
//!
//! # Architectural Invariants
//!
//! 1. **Thread-Safe Selection**: Endpoint selection must be thread-safe and highly concurrent.
//! 2. **Atomic Updates**: Dynamic updates to upstream state must be atomic or eventually consistent without blocking readers.
//! 3. **Distributed State**: Load balancing state (e.g., RR counters) should be distributed or aligned to prevent false sharing.

use crate::config::Upstream;
use std::collections::HashMap;

pub mod cluster;
pub mod load_balance;

pub use cluster::Cluster;

pub struct Manager {
    clusters: HashMap<String, Cluster>,
}

impl Manager {
    pub fn new(upstreams: &[Upstream]) -> Self {
        let mut clusters = HashMap::new();
        for u in upstreams {
            clusters.insert(u.name.clone(), Cluster::new(u.clone()));
        }
        Self { clusters }
    }

    pub fn get(&self, name: &str) -> Option<&Cluster> {
        self.clusters.get(name)
    }
}
