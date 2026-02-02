use super::client_identity::ClientIdentityMaterials;
use super::load_balance;
use arc_swap::ArcSwap;
use pavis_core::{
    ActiveHealthCheck, CircuitBreakerPolicy, Endpoint, EndpointAddr, OutlierDetectionPolicy,
    Upstream,
};
use pingora::protocols::tls::CaType;
use pingora::utils::tls::CertKey;
use std::sync::{Arc, Mutex};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

mod health;
mod pool;
mod state;
mod tls;

use health::HealthState;
use pool::PoolController;
use state::{AlignedCounter, ClusterState, build_state_parts};
use tls::TlsMaterials;

pub use pool::{PoolPermit, PoolRejection};

#[derive(Debug)]
enum CircuitBreaker {
    Disabled,
    Enabled {
        max_connections: Arc<Semaphore>,
        max_pending_requests: Arc<Semaphore>,
    },
}

#[derive(Debug)]
pub enum CircuitBreakerRejection {
    PendingLimit,
    Closed,
}

#[derive(Debug, Clone, Copy)]
pub enum UpstreamOutcome {
    Success,
    Failure,
}

#[derive(Debug)]
pub struct Cluster {
    pub(crate) config: Upstream,
    pub(crate) rr_counter: AlignedCounter,
    state: ArcSwap<ClusterState>,
    health: Mutex<HealthState>,
    pool: PoolController,
    breaker: CircuitBreaker,
    tls: TlsMaterials,
}

impl Cluster {
    pub fn new(config: Upstream) -> Self {
        Self::new_with_tls_materials(config, ClientIdentityMaterials::default())
    }

    pub fn new_with_tls_materials(config: Upstream, materials: ClientIdentityMaterials) -> Self {
        let ClientIdentityMaterials {
            client_cert_key,
            ca_bundle,
            health_identity,
            health_root_certificates,
        } = materials;
        let health = HealthState::new(config.endpoints.clone());
        let eligible = health.eligible_endpoints();
        let (endpoints, cumulative_weights, total_weight) = build_state_parts(eligible);
        let state = ClusterState {
            endpoints,
            cumulative_weights,
            total_weight,
        };
        let pool = PoolController::new(&config);
        let breaker = match config.circuit_breaker {
            CircuitBreakerPolicy::Disabled => CircuitBreaker::Disabled,
            CircuitBreakerPolicy::Enabled {
                max_connections,
                max_pending_requests,
            } => CircuitBreaker::Enabled {
                max_connections: Arc::new(Semaphore::new(max_connections.0.get() as usize)),
                max_pending_requests: Arc::new(Semaphore::new(
                    max_pending_requests.0.get() as usize
                )),
            },
            #[allow(unreachable_patterns)]
            _ => CircuitBreaker::Disabled,
        };

        Self {
            config,
            rr_counter: AlignedCounter(std::sync::atomic::AtomicUsize::new(0)),
            state: ArcSwap::from_pointee(state),
            health: Mutex::new(health),
            pool,
            breaker,
            tls: TlsMaterials::new(
                client_cert_key,
                ca_bundle,
                health_identity,
                health_root_certificates,
            ),
        }
    }

    pub fn client_cert_key(&self) -> Option<Arc<CertKey>> {
        self.tls.client_cert_key()
    }

    pub fn ca_bundle(&self) -> Option<Arc<CaType>> {
        self.tls.ca_bundle()
    }

    pub fn health_identity(&self) -> Option<Arc<reqwest::Identity>> {
        self.tls.health_identity()
    }

    pub fn health_root_certificates(&self) -> Arc<Vec<reqwest::Certificate>> {
        self.tls.health_root_certificates()
    }

    pub async fn acquire_pool_permit(&self) -> Result<Option<PoolPermit>, PoolRejection> {
        self.pool.acquire().await.map(Some)
    }

    pub fn select_endpoint(&self) -> Option<Endpoint> {
        self.refresh_expired_ejections();
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
        let mut health = self.health.lock().expect("health lock poisoned");
        health.update_endpoints(endpoints);
        self.refresh_state_from_health(&mut health);
    }

    pub fn current_endpoints(&self) -> Vec<Endpoint> {
        self.health
            .lock()
            .expect("health lock poisoned")
            .clone_endpoints()
    }

    pub fn set_active_health(&self, endpoint: &EndpointAddr, healthy: bool) {
        if !matches!(self.config.health_check, ActiveHealthCheck::Enabled { .. }) {
            return;
        }
        let mut health = self.health.lock().expect("health lock poisoned");
        let changed = health.mark_active_health(endpoint, healthy);
        if changed {
            self.refresh_state_from_health(&mut health);
        }
    }

    pub fn record_outcome(&self, endpoint: &EndpointAddr, outcome: UpstreamOutcome) {
        let (threshold, eject_duration) = match self.config.outlier_detection {
            OutlierDetectionPolicy::Enabled {
                consecutive_errors,
                eject_duration,
            } => (
                consecutive_errors.0.get(),
                std::time::Duration::from_millis(eject_duration.0.get() as u64),
            ),
            OutlierDetectionPolicy::Disabled => return,
            #[allow(unreachable_patterns)]
            _ => return,
        };

        let mut health = self.health.lock().expect("health lock poisoned");
        let changed = match outcome {
            UpstreamOutcome::Success => health.record_success(endpoint),
            UpstreamOutcome::Failure => health.record_failure(endpoint, threshold, eject_duration),
        };
        if changed {
            self.refresh_state_from_health(&mut health);
        }
    }

    pub async fn acquire_breaker_permit(
        &self,
    ) -> Result<Option<OwnedSemaphorePermit>, CircuitBreakerRejection> {
        match &self.breaker {
            CircuitBreaker::Disabled => Ok(None),
            CircuitBreaker::Enabled {
                max_connections,
                max_pending_requests,
            } => {
                if let Ok(permit) = max_connections.clone().try_acquire_owned() {
                    return Ok(Some(permit));
                }
                let pending = max_pending_requests
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| CircuitBreakerRejection::PendingLimit)?;
                let permit = max_connections
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| CircuitBreakerRejection::Closed)?;
                drop(pending);
                Ok(Some(permit))
            }
        }
    }

    fn refresh_expired_ejections(&self) {
        let mut health = self.health.lock().expect("health lock poisoned");
        if health.clear_expired_ejections() {
            self.refresh_state_from_health(&mut health);
        }
    }

    fn refresh_state_from_health(&self, health: &mut HealthState) {
        health.clear_expired_ejections();
        let eligible = health.eligible_endpoints();
        let (endpoints, cumulative_weights, total_weight) = build_state_parts(eligible);
        let state = ClusterState {
            endpoints,
            cumulative_weights,
            total_weight,
        };
        self.state.store(Arc::new(state));
    }
}

#[cfg(test)]
mod tests {
    use super::{Cluster, PoolRejection};
    use pavis_core::{
        ActiveHealthCheck, CircuitBreakerPolicy, ConnectTimeout, ConnectionLimit,
        ConsecutiveErrors, Discovery, Duration, Endpoint, EndpointAddr, HttpVersion, IdleTimeout,
        LoadBalancer, MaxConnections, MaxPendingRequests, OutlierDetectionPolicy, Pool, PoolQueue,
        Port, TlsPolicy, UpstreamBuilder, UpstreamId, UpstreamName, Weight,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::{NonZeroU16, NonZeroU32};
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

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
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
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
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
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
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
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

        let count = cluster
            .rr_counter
            .0
            .load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(count, 1000);
    }

    #[test]
    fn outlier_detection_ejects_and_reenables_endpoint() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("outlier".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .outlier_detection(OutlierDetectionPolicy::Enabled {
                consecutive_errors: ConsecutiveErrors(NonZeroU32::new(2).unwrap()),
                eject_duration: Duration(NonZeroU32::new(10).unwrap()),
            })
            .circuit_breaker(CircuitBreakerPolicy::Disabled)
            .health_check(ActiveHealthCheck::Disabled)
            .tls(TlsPolicy::Disabled)
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8080, 1))
            .build()
            .expect("upstream");

        let cluster = Cluster::new(upstream);
        let endpoint = cluster.select_endpoint().expect("endpoint");
        cluster.record_outcome(&endpoint.address, super::UpstreamOutcome::Failure);
        cluster.record_outcome(&endpoint.address, super::UpstreamOutcome::Failure);

        assert!(cluster.select_endpoint().is_none());

        std::thread::sleep(StdDuration::from_millis(20));
        assert!(cluster.select_endpoint().is_some());
    }

    #[tokio::test]
    async fn circuit_breaker_rejects_when_pending_full() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("breaker".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
            })
            .outlier_detection(OutlierDetectionPolicy::Disabled)
            .circuit_breaker(CircuitBreakerPolicy::Enabled {
                max_connections: MaxConnections(NonZeroU32::new(1).unwrap()),
                max_pending_requests: MaxPendingRequests(NonZeroU32::new(1).unwrap()),
            })
            .health_check(ActiveHealthCheck::Disabled)
            .tls(TlsPolicy::Disabled)
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 8080, 1))
            .build()
            .expect("upstream");

        let cluster = Arc::new(Cluster::new(upstream));
        let _permit = cluster
            .acquire_breaker_permit()
            .await
            .expect("permit")
            .expect("permit");

        let pending_cluster = cluster.clone();
        let pending = tokio::spawn(async move { pending_cluster.acquire_breaker_permit().await });

        tokio::time::sleep(StdDuration::from_millis(10)).await;
        let third = cluster.acquire_breaker_permit().await;
        assert!(matches!(
            third,
            Err(super::CircuitBreakerRejection::PendingLimit)
        ));

        pending.abort();
    }

    #[tokio::test]
    async fn pool_rejects_when_queue_disabled() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("pool-limit".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(1).unwrap()),
                queue: PoolQueue {
                    capacity: 0,
                    timeout_ms: 0,
                },
                tcp_keepalive: None,
                tcp_nodelay: None,
                recv_buffer_size: None,
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 9000, 1))
            .build()
            .expect("upstream");

        let cluster = Cluster::new(upstream);
        let permit = cluster
            .acquire_pool_permit()
            .await
            .expect("pool result")
            .expect("permit");
        let second = cluster.acquire_pool_permit().await;
        assert!(matches!(second, Err(PoolRejection::QueueFull)));
        drop(permit);
    }

    #[tokio::test]
    async fn pool_waiter_times_out_when_no_slot_opens() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("pool-timeout".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(1).unwrap()),
                queue: PoolQueue {
                    capacity: 1,
                    timeout_ms: 50,
                },
                tcp_keepalive: None,
                tcp_nodelay: None,
                recv_buffer_size: None,
            })
            .tls(TlsPolicy::Disabled)
            .add_endpoint(make_endpoint(Ipv4Addr::new(127, 0, 0, 1), 9001, 1))
            .build()
            .expect("upstream");

        let cluster = Cluster::new(upstream);
        let permit = cluster
            .acquire_pool_permit()
            .await
            .expect("pool result")
            .expect("permit");
        let second = cluster.acquire_pool_permit().await;
        assert!(matches!(second, Err(PoolRejection::QueueTimeout)));
        drop(permit);
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
                max: ConnectionLimit(NonZeroU32::new(128).unwrap()),
                ..Pool::default()
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
