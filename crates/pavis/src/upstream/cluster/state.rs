use pavis_core::Endpoint;
use std::sync::atomic::AtomicUsize;

#[repr(align(64))]
#[derive(Debug)]
pub(crate) struct AlignedCounter(pub AtomicUsize);

#[derive(Debug)]
pub(crate) struct ClusterState {
    pub(crate) endpoints: Vec<Endpoint>,
    pub(crate) cumulative_weights: Vec<u32>,
    pub(crate) total_weight: u32,
}

pub(crate) fn build_state_parts(endpoints: Vec<Endpoint>) -> (Vec<Endpoint>, Vec<u32>, u32) {
    let mut cumulative_weights = Vec::with_capacity(endpoints.len());
    let mut sum = 0u32;
    for e in &endpoints {
        sum += e.weight.0.get() as u32;
        cumulative_weights.push(sum);
    }
    (endpoints, cumulative_weights, sum)
}
