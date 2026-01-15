use super::load_balance;
use arc_swap::ArcSwap;
use pavis_core::{Endpoint, Upstream};
use pingora::protocols::tls::CaType;
use pingora::utils::tls::CertKey;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

#[repr(align(64))]
#[derive(Debug)]
pub(crate) struct AlignedCounter(pub AtomicUsize);

#[derive(Debug)]
struct ClusterState {
    endpoints: Vec<Endpoint>,
    cumulative_weights: Vec<u32>,
    total_weight: u32,
}

#[derive(Debug)]
pub struct Cluster {
    pub(crate) config: Upstream,
    // Co-located state
    pub(crate) rr_counter: AlignedCounter,
    state: ArcSwap<ClusterState>,
    client_cert_key: Option<Arc<CertKey>>,
    ca_bundle: Option<Arc<CaType>>,
}

impl Cluster {
    pub fn new(config: Upstream) -> Self {
        Self::new_with_client_cert(config, None, None)
    }

    pub fn new_with_client_cert(
        config: Upstream,
        client_cert_key: Option<Arc<CertKey>>,
        ca_bundle: Option<Arc<CaType>>,
    ) -> Self {
        let (endpoints, cumulative_weights, total_weight) =
            build_state_parts(config.endpoints.clone());
        let state = ClusterState {
            endpoints,
            cumulative_weights,
            total_weight,
        };
        Self {
            config,
            rr_counter: AlignedCounter(AtomicUsize::new(0)),
            state: ArcSwap::from_pointee(state),
            client_cert_key,
            ca_bundle,
        }
    }

    pub fn client_cert_key(&self) -> Option<Arc<CertKey>> {
        self.client_cert_key.clone()
    }

    pub fn ca_bundle(&self) -> Option<Arc<CaType>> {
        self.ca_bundle.clone()
    }

    pub fn select_endpoint(&self) -> Option<Endpoint> {
        let state = self.state.load();
        if state.endpoints.is_empty() {
            return None;
        }
        let idx = load_balance::select_index(
            self.config.balancer,
            &state.endpoints,
            &state.cumulative_weights,
            &self.rr_counter.0,
            state.total_weight,
        );
        state.endpoints.get(idx).cloned()
    }

    pub fn update_endpoints(&self, endpoints: Vec<Endpoint>) {
        let (endpoints, cumulative_weights, total_weight) = build_state_parts(endpoints);
        let state = ClusterState {
            endpoints,
            cumulative_weights,
            total_weight,
        };
        self.state.store(Arc::new(state));
    }

    pub fn current_endpoints(&self) -> Vec<Endpoint> {
        self.state.load().endpoints.clone()
    }
}

fn build_state_parts(endpoints: Vec<Endpoint>) -> (Vec<Endpoint>, Vec<u32>, u32) {
    let mut cumulative_weights = Vec::with_capacity(endpoints.len());
    let mut sum = 0u32;
    for e in &endpoints {
        sum += e.weight.0.get() as u32;
        cumulative_weights.push(sum);
    }
    (endpoints, cumulative_weights, sum)
}

#[cfg(test)]
mod tests {
    use super::Cluster;
    use pavis_core::{
        ConnectTimeout, ConnectionLimit, Discovery, Endpoint, EndpointAddr, HttpVersion,
        IdleTimeout, LoadBalancer, Pool, Port, TlsPolicy, UpstreamBuilder, UpstreamId,
        UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU16;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    fn make_endpoint(ip: Ipv4Addr, port: u16, weight: u16) -> Endpoint {
        Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(ip),
                port: Port(NonZeroU16::new(port).unwrap()),
            },
            weight: Weight(NonZeroU16::new(weight).unwrap()),
        }
    }

    fn get_ip(e: &Endpoint) -> IpAddr {
        match e.address {
            EndpointAddr::Ip { address, .. } => address,
            _ => panic!("expected ip"),
        }
    }

    fn get_port(e: &Endpoint) -> u16 {
        match e.address {
            EndpointAddr::Ip { port, .. } => port.0.get(),
            _ => panic!("expected ip"),
        }
    }

    #[test]
    fn test_weighted_round_robin_respects_weights() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8080, 3))
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 2), 8081, 1))
            .build()
            .expect("upstream");

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
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test-upstream".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8081, 1))
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8082, 1))
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8083, 1))
            .build()
            .expect("upstream");

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
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("concurrent-upstream".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 80, 1))
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 2), 80, 1))
            .build()
            .expect("upstream");

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

    #[test]
    fn test_cluster_update_endpoints() {
        let u = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit::Unlimited,
            })
            .tls(TlsPolicy::Disabled)
            .build()
            .expect("upstream");
        let cluster = Cluster::new(u);
        assert!(cluster.current_endpoints().is_empty());

        let ep = Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(8080).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        };
        cluster.update_endpoints(vec![ep.clone()]);

        let current = cluster.current_endpoints();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].address, ep.address);
    }
}
