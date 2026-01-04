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
            clusters.insert(u.name.0.clone(), Cluster::new(u.clone()));
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
        ConnectTimeout, ConnectionLimit, Endpoint, EndpointAddr, HttpVersion, IdleTimeout,
        LoadBalancer, Pool, Port, TlsPolicy, Upstream, UpstreamId, UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU16;

    #[test]
    fn manager_returns_configured_cluster() {
        let upstreams = vec![Upstream {
            id: UpstreamId(NonZeroU16::new(1).unwrap()),
            name: UpstreamName("backend".to_string()),
            discovery: pavis_core::Discovery::Static,
            balancer: LoadBalancer::RoundRobin,
            protocol: HttpVersion::H1,
            pool: Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            },
            tls: TlsPolicy::Disabled,
            endpoints: vec![Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            }],
        }];

        let manager = Manager::new(&upstreams);
        let cluster = manager.get("backend");
        assert!(cluster.is_some());
        assert_eq!(cluster.unwrap().config.name.0, "backend");
    }
}
