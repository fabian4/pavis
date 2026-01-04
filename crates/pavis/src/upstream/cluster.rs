use super::load_balance;
use arc_swap::ArcSwap;
use pavis_core::{Endpoint, Upstream};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

#[repr(align(64))]
#[derive(Debug)]
pub(crate) struct AlignedCounter(pub AtomicUsize);

#[derive(Debug)]
struct ClusterState {
    endpoints: Vec<Endpoint>,
    total_weight: u32,
}

#[derive(Debug)]
pub struct Cluster {
    pub(crate) config: Upstream,
    // Co-located state
    pub(crate) rr_counter: AlignedCounter,
    state: ArcSwap<ClusterState>,
}

impl Cluster {
    pub fn new(config: Upstream) -> Self {
        let total_weight = config.endpoints.iter().map(|e| e.weight).sum();
        let state = ClusterState {
            endpoints: config.endpoints.clone(),
            total_weight,
        };
        Self {
            config,
            rr_counter: AlignedCounter(AtomicUsize::new(0)),
            state: ArcSwap::from_pointee(state),
        }
    }

    pub fn select_endpoint(&self) -> Option<Endpoint> {
        let state = self.state.load();
        if state.endpoints.is_empty() {
            return None;
        }
        let idx = load_balance::select_index(
            self.config.load_balancer,
            &state.endpoints,
            &self.rr_counter.0,
            state.total_weight,
        );
        state.endpoints.get(idx).cloned()
    }

    pub fn update_endpoints(&self, endpoints: Vec<Endpoint>) {
        let total_weight = endpoints.iter().map(|e| e.weight).sum();
        let state = ClusterState {
            endpoints,
            total_weight,
        };
        self.state.store(Arc::new(state));
    }

    pub fn current_endpoints(&self) -> Vec<Endpoint> {
        self.state.load().endpoints.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::Cluster;
    use pavis_core::{
        ConnectionPoolConfig, Endpoint, EndpointAddress, HttpVersion, LoadBalancer, Upstream,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    fn make_endpoint(ip: Ipv4Addr, port: u16, weight: u32) -> Endpoint {
        Endpoint {
            address: EndpointAddress::Ip(SocketAddr::new(IpAddr::V4(ip), port)),
            weight,
        }
    }

    fn get_ip(e: &Endpoint) -> IpAddr {
        match e.address {
            EndpointAddress::Ip(addr) => addr.ip(),
            _ => panic!("expected ip"),
        }
    }

    fn get_port(e: &Endpoint) -> u16 {
        match e.address {
            EndpointAddress::Ip(addr) => addr.port(),
            _ => panic!("expected ip"),
        }
    }

    #[test]
    fn test_weighted_round_robin_respects_weights() {
        let upstream = Upstream {
            name: "test".to_string(),
            discovery_type: pavis_core::DiscoveryType::Static,
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![
                make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8080, 3),
                make_endpoint(Ipv4Addr::new(127, 0, 0, 2), 8081, 1),
            ],
        };

        let cluster = Cluster::new(upstream);

        let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

        assert_eq!(get_ip(&cluster.select_endpoint().unwrap()), ip1);
        assert_eq!(get_ip(&cluster.select_endpoint().unwrap()), ip1);
        assert_eq!(get_ip(&cluster.select_endpoint().unwrap()), ip1);
        assert_eq!(get_ip(&cluster.select_endpoint().unwrap()), ip2);
        assert_eq!(get_ip(&cluster.select_endpoint().unwrap()), ip1);
    }

    #[test]
    fn test_round_robin_cycles_endpoints_evenly() {
        let upstream = Upstream {
            name: "test-upstream".to_string(),
            discovery_type: pavis_core::DiscoveryType::Static,
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![
                make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8081, 1),
                make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8082, 1),
                make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8083, 1),
            ],
        };

        let cluster = Cluster::new(upstream);

        let e1 = cluster.select_endpoint().unwrap();
        assert_eq!(get_port(&e1), 8081);

        let e2 = cluster.select_endpoint().unwrap();
        assert_eq!(get_port(&e2), 8082);

        let e3 = cluster.select_endpoint().unwrap();
        assert_eq!(get_port(&e3), 8083);

        let e4 = cluster.select_endpoint().unwrap();
        assert_eq!(get_port(&e4), 8081);
    }

    #[test]
    fn test_concurrent_round_robin() {
        let upstream = Upstream {
            name: "concurrent-upstream".to_string(),
            discovery_type: pavis_core::DiscoveryType::Static,
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![
                make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 80, 1),
                make_endpoint(Ipv4Addr::new(127, 0, 0, 2), 80, 1),
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
