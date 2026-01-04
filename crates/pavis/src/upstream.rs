//! Upstream module: Backend cluster management and load balancing.
//!
//! # Architectural Invariants
//!
//! 1. **Thread-Safe Selection**: Endpoint selection must be thread-safe and highly concurrent.
//! 2. **Atomic Updates**: Dynamic updates to upstream state must be atomic or eventually consistent without blocking readers.
//! 3. **Distributed State**: Load balancing state (e.g., RR counters) should be distributed or aligned to prevent false sharing.

use std::collections::HashMap;

use pavis_core::Upstream;

pub mod cluster;
pub mod load_balance;
pub mod resolver;

pub use cluster::Cluster;
pub use resolver::UpstreamResolver;

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

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Cluster)> {
        self.clusters.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::Manager;
    use pavis_core::{
        ConnectionPoolConfig, Endpoint, EndpointAddress, HttpVersion, LoadBalancer, Upstream,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn manager_returns_configured_cluster() {
        let upstreams = vec![Upstream {
            name: "backend".to_string(),
            discovery_type: pavis_core::DiscoveryType::Static,
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
            endpoints: vec![Endpoint {
                address: EndpointAddress::Ip(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    8080,
                )),
                weight: 1,
            }],
        }];

        let manager = Manager::new(&upstreams);
        let cluster = manager.get("backend");
        assert!(cluster.is_some());
        assert_eq!(cluster.unwrap().config.name, "backend");
    }
}
