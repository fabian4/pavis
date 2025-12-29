use super::load_balance;
use pavis_core::{Endpoint, Upstream};
use std::sync::atomic::AtomicUsize;

#[repr(align(64))]
#[derive(Debug)]
pub(crate) struct AlignedCounter(pub AtomicUsize);

#[derive(Debug)]
pub struct Cluster {
    pub(crate) config: Upstream,
    // Co-located state
    pub(crate) rr_counter: AlignedCounter,
    pub(crate) total_weight: u32,
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

#[cfg(test)]
mod tests {
    use super::Cluster;
    use pavis_core::{ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, Upstream};
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_weighted_round_robin_respects_weights() {
        let upstream = Upstream {
            name: "test".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![
                Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: 8080,
                    weight: 3,
                },
                Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                    port: 8081,
                    weight: 1,
                },
            ],
        };

        let cluster = Cluster::new(upstream);

        let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

        assert_eq!(cluster.select_endpoint().unwrap().ip, ip1);
        assert_eq!(cluster.select_endpoint().unwrap().ip, ip1);
        assert_eq!(cluster.select_endpoint().unwrap().ip, ip1);
        assert_eq!(cluster.select_endpoint().unwrap().ip, ip2);
        assert_eq!(cluster.select_endpoint().unwrap().ip, ip1);
    }

    #[test]
    fn test_round_robin_cycles_endpoints_evenly() {
        let upstream = Upstream {
            name: "test-upstream".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![
                Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: 8081,
                    weight: 1,
                },
                Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: 8082,
                    weight: 1,
                },
                Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: 8083,
                    weight: 1,
                },
            ],
        };

        let cluster = Cluster::new(upstream);

        let e1 = cluster.select_endpoint().unwrap();
        assert_eq!(e1.port, 8081);

        let e2 = cluster.select_endpoint().unwrap();
        assert_eq!(e2.port, 8082);

        let e3 = cluster.select_endpoint().unwrap();
        assert_eq!(e3.port, 8083);

        let e4 = cluster.select_endpoint().unwrap();
        assert_eq!(e4.port, 8081);
    }

    #[test]
    fn test_concurrent_round_robin() {
        let upstream = Upstream {
            name: "concurrent-upstream".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![
                Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: 80,
                    weight: 1,
                },
                Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                    port: 80,
                    weight: 1,
                },
            ],
        };

        let cluster = Arc::new(Cluster::new(upstream));

        let mut handles = vec![];
        for _ in 0..10 {
            let c = cluster.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    let _ = c.select_endpoint();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let count = cluster.rr_counter.0.load(Ordering::Relaxed);
        assert_eq!(count, 1000);
    }
}
