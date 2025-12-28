use pavis_core::{Endpoint, LoadBalancer};
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
                let w = endpoint.weight;
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
                let w = endpoint.weight;
                if pick < w {
                    return i;
                }
                pick -= w;
            }
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::select_index;
    use pavis_core::{Endpoint, LoadBalancer};
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn select_index_returns_zero_for_empty_or_zero_weight() {
        let counter = AtomicUsize::new(0);
        let idx = select_index(LoadBalancer::RoundRobin, &[], &counter, 0);
        assert_eq!(idx, 0);

        let endpoints = vec![Endpoint {
            ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            weight: 0,
        }];
        let idx = select_index(LoadBalancer::Random, &endpoints, &counter, 0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn select_index_round_robin_respects_weights() {
        let counter = AtomicUsize::new(0);
        let endpoints = vec![
            Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: 8080,
                weight: 2,
            },
            Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                port: 8081,
                weight: 1,
            },
        ];
        let total_weight = 3;
        assert_eq!(
            select_index(LoadBalancer::RoundRobin, &endpoints, &counter, total_weight),
            0
        );
        assert_eq!(
            select_index(LoadBalancer::RoundRobin, &endpoints, &counter, total_weight),
            0
        );
        assert_eq!(
            select_index(LoadBalancer::RoundRobin, &endpoints, &counter, total_weight),
            1
        );
    }

    #[test]
    fn select_index_random_is_in_range() {
        let counter = AtomicUsize::new(0);
        let endpoints = vec![
            Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: 8080,
                weight: 1,
            },
            Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                port: 8081,
                weight: 1,
            },
        ];
        let total_weight = 2;
        for _ in 0..10 {
            let idx = select_index(LoadBalancer::Random, &endpoints, &counter, total_weight);
            assert!(idx < endpoints.len());
        }
    }
}
