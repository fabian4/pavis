use pavis_core::config::{Endpoint, LoadBalancer};
use rand::Rng;
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn select_index(
    lb: LoadBalancer,
    endpoints: &[Endpoint],
    counter: &AtomicUsize,
    total_weight: u32,
) -> usize {
    if endpoints.is_empty() || total_weight == 0 {
        return 0;
    }

    match lb {
        LoadBalancer::RoundRobin => {
            let val = counter.fetch_add(1, Ordering::Relaxed);
            let mut current = (val as u32) % total_weight;

            for (i, endpoint) in endpoints.iter().enumerate() {
                let w = endpoint.weight.unwrap_or(1);
                if current < w {
                    return i;
                }
                current -= w;
            }
            0
        }
        LoadBalancer::Random => {
            let mut rng = rand::rng();
            let mut pick = rng.random_range(0..total_weight);

            for (i, endpoint) in endpoints.iter().enumerate() {
                let w = endpoint.weight.unwrap_or(1);
                if pick < w {
                    return i;
                }
                pick -= w;
            }
            0
        }
    }
}
