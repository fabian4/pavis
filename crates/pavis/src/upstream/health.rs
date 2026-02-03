//! Active health checks for upstream endpoints.

use anyhow::{Context, Result};
use async_trait::async_trait;
use pavis_core::{ActiveHealthCheck, Endpoint, EndpointAddr, SniName, TlsPolicy, TlsVerify};
use pingora::services::Service;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

use crate::state::RuntimeStateHandle;

use super::Manager;
use super::cluster::Cluster;

pub struct UpstreamHealthMonitor {
    state: Arc<RuntimeStateHandle>,
}

impl UpstreamHealthMonitor {
    pub fn new(state: Arc<RuntimeStateHandle>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Service for UpstreamHealthMonitor {
    async fn start_service(
        &mut self,
        _fds: Option<Arc<tokio::sync::Mutex<pingora::server::Fds>>>,
        mut shutdown: watch::Receiver<bool>,
        _threads: usize,
    ) {
        let mut last_state_ptr = 0usize;
        let mut scheduler = Scheduler::default();
        let executor = Executor;
        let mut tick = tokio::time::interval(Duration::from_millis(200));

        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tick.tick() => {
                    let state = self.state.load();
                    let state_ptr = Arc::as_ptr(&state) as usize;
                    if state_ptr != last_state_ptr {
                        scheduler = Scheduler::from_manager(&state.upstream_manager);
                        last_state_ptr = state_ptr;
                    }

                    let jobs = scheduler.next_jobs(Instant::now());
                    executor.dispatch(jobs);
                }
            }
        }
    }

    fn name(&self) -> &str {
        "upstream_health_monitor"
    }
}
#[derive(Clone)]
struct HealthProbePlan {
    upstream: Arc<str>,
    path: Arc<str>,
    interval: Duration,
    scheme: &'static str,
    client: Client,
}

impl HealthProbePlan {
    fn build(name: &str, cluster: &Arc<Cluster>) -> Result<Option<Self>> {
        let (path, interval, timeout) = match &cluster.config.health_check {
            ActiveHealthCheck::Enabled {
                path,
                interval,
                timeout,
            } => (
                Arc::<str>::from(path.0.clone()),
                core_duration_to_std(interval),
                core_duration_to_std(timeout),
            ),
            ActiveHealthCheck::Disabled => return Ok(None),
            #[allow(unreachable_patterns)]
            _ => return Ok(None),
        };

        let scheme = match cluster.config.tls {
            TlsPolicy::Disabled => "http",
            TlsPolicy::Enabled { .. } => "https",
            #[allow(unreachable_patterns)]
            _ => "http",
        };

        let client = build_health_client(
            &cluster.config,
            timeout,
            cluster.health_identity(),
            cluster.health_root_certificates(),
        )?;

        Ok(Some(Self {
            upstream: Arc::from(name.to_string()),
            path,
            interval,
            scheme,
            client,
        }))
    }

    fn upstream(&self) -> &str {
        &self.upstream
    }

    fn interval(&self) -> Duration {
        self.interval
    }

    async fn probe(&self, cluster: &Cluster, endpoint: &Endpoint) -> Result<bool> {
        let (host, port) = match &endpoint.address {
            EndpointAddr::Ip { address, port } => (address.to_string(), port.0.get()),
            EndpointAddr::Dns { host, port } => (host.0.clone(), port.0.get()),
            #[allow(unreachable_patterns)]
            _ => ("127.0.0.1".to_string(), 80),
        };
        let url = format!("{}://{}:{}{}", self.scheme, host, port, self.path);
        let mut request = self.client.get(url);
        if let Some(host_header) = health_check_host(&cluster.config, &endpoint.address) {
            request = request.header(reqwest::header::HOST, host_header);
        }
        let response = request
            .send()
            .await
            .context("health probe request failed")?;
        Ok(response.status().is_success())
    }
}

struct PlanState {
    plan: Arc<HealthProbePlan>,
    cluster: Arc<Cluster>,
    next_due: Instant,
}

impl PlanState {
    fn is_due(&self, now: Instant) -> bool {
        now >= self.next_due
    }
}

#[derive(Default)]
struct Scheduler {
    plans: HashMap<String, PlanState>,
}

impl Scheduler {
    fn from_manager(manager: &Manager) -> Self {
        let mut plans = HashMap::new();
        let now = Instant::now();
        for (name, cluster) in manager.iter() {
            match HealthProbePlan::build(name, &cluster) {
                Ok(Some(plan)) => {
                    let next_due = now + jitter_duration(plan.interval());
                    plans.insert(
                        name.clone(),
                        PlanState {
                            plan: Arc::new(plan),
                            cluster,
                            next_due,
                        },
                    );
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(upstream = %name, error = %err, "failed to build health probe plan");
                    let endpoints = cluster.current_endpoints();
                    mark_all_unhealthy(&cluster, &endpoints);
                }
            }
        }
        Self { plans }
    }

    fn next_jobs(&mut self, now: Instant) -> Vec<ProbeJob> {
        let mut jobs = Vec::new();
        for state in self.plans.values_mut() {
            if state.is_due(now) {
                state.next_due =
                    now + state.plan.interval() + jitter_duration(state.plan.interval());
                jobs.push(ProbeJob::new(state.plan.clone(), state.cluster.clone()));
            }
        }
        jobs
    }
}

struct ProbeJob {
    plan: Arc<HealthProbePlan>,
    cluster: Arc<Cluster>,
}

impl ProbeJob {
    fn new(plan: Arc<HealthProbePlan>, cluster: Arc<Cluster>) -> Self {
        Self { plan, cluster }
    }

    async fn run(self) {
        let endpoints = self.cluster.current_endpoints();
        if endpoints.is_empty() {
            return;
        }

        for endpoint in endpoints {
            let healthy = match self.plan.probe(&self.cluster, &endpoint).await {
                Ok(result) => result,
                Err(err) => {
                    tracing::debug!(
                        upstream = self.plan.upstream(),
                        endpoint = %endpoint_label(&endpoint.address),
                        error = %err,
                        "health probe failed"
                    );
                    false
                }
            };
            self.cluster.set_active_health(&endpoint.address, healthy);
        }
    }
}

struct Executor;

impl Executor {
    fn dispatch(&self, jobs: Vec<ProbeJob>) {
        for job in jobs {
            tokio::spawn(async move {
                job.run().await;
            });
        }
    }
}

fn mark_all_unhealthy(cluster: &Cluster, endpoints: &[Endpoint]) {
    for endpoint in endpoints {
        cluster.set_active_health(&endpoint.address, false);
    }
}

fn core_duration_to_std(duration: &pavis_core::Duration) -> Duration {
    Duration::from_millis(duration.0.get() as u64)
}

fn jitter_duration(base: Duration) -> Duration {
    let jitter_ms = (base.as_millis() / 10).min(50);
    if jitter_ms == 0 {
        return Duration::ZERO;
    }
    let offset = rand::random::<u128>() % (jitter_ms + 1);
    Duration::from_millis(offset as u64)
}

fn endpoint_label(addr: &EndpointAddr) -> String {
    match addr {
        EndpointAddr::Ip { address, port } => format!("{}:{}", address, port.0.get()),
        EndpointAddr::Dns { host, port } => format!("{}:{}", host.0, port.0.get()),
        #[allow(unreachable_patterns)]
        _ => "unknown".to_string(),
    }
}
fn health_check_host(upstream: &pavis_core::Upstream, endpoint: &EndpointAddr) -> Option<String> {
    match &upstream.tls {
        TlsPolicy::Enabled {
            sni,
            canonical_sni,
            reuse_across_sni,
            ..
        } => match canonical_sni {
            pavis_core::CanonicalSni::Enabled { name } => Some(name.0.clone()),
            pavis_core::CanonicalSni::Disabled => match reuse_across_sni {
                pavis_core::ReuseAcrossSni::Enabled => match sni {
                    SniName::Name(name) => Some(name.0.clone()),
                    SniName::Auto => match endpoint {
                        EndpointAddr::Dns { host, .. } => Some(host.0.clone()),
                        _ => None,
                    },
                    SniName::Disabled => None,
                    #[allow(unreachable_patterns)]
                    _ => None,
                },
                pavis_core::ReuseAcrossSni::Disabled => match sni {
                    SniName::Name(name) => Some(name.0.clone()),
                    SniName::Auto => match endpoint {
                        EndpointAddr::Dns { host, .. } => Some(host.0.clone()),
                        _ => None,
                    },
                    SniName::Disabled => None,
                    #[allow(unreachable_patterns)]
                    _ => None,
                },
                #[allow(unreachable_patterns)]
                _ => None,
            },
            #[allow(unreachable_patterns)]
            _ => None,
        },
        TlsPolicy::Disabled => match endpoint {
            EndpointAddr::Dns { host, .. } => Some(host.0.clone()),
            _ => None,
        },
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn build_health_client(
    upstream: &pavis_core::Upstream,
    timeout: Duration,
    identity: Option<Arc<reqwest::Identity>>,
    root_certificates: Arc<Vec<reqwest::Certificate>>,
) -> Result<Client> {
    let mut builder = Client::builder().timeout(timeout).connect_timeout(timeout);

    if let TlsPolicy::Enabled { verify, .. } = &upstream.tls {
        match verify {
            TlsVerify::Disabled => {
                builder = builder
                    .danger_accept_invalid_certs(true)
                    .danger_accept_invalid_hostnames(true);
            }
            TlsVerify::CaOnly => {
                builder = builder.danger_accept_invalid_hostnames(true);
            }
            TlsVerify::Full => {}
            #[allow(unreachable_patterns)]
            _ => {}
        }

        if let Some(identity) = identity {
            builder = builder.identity((*identity).clone());
        }

        for cert in root_certificates.iter() {
            builder = builder.add_root_certificate(cert.clone());
        }
    }

    builder
        .build()
        .context("failed to build health check client")
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream::Manager;
    use pavis_core::{
        Discovery, EndpointAddr, Hostname, HttpVersion, LoadBalancer, Pool, Port, TlsPolicy,
        Upstream, UpstreamBuilder, UpstreamId, UpstreamName,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::{NonZeroU16, NonZeroU32};

    fn make_upstream(tls: TlsPolicy, health: ActiveHealthCheck) -> Upstream {
        UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .pool(Pool::default())
            .tls(tls)
            .health_check(health)
            .add_endpoint(pavis_core::Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(8080).unwrap()),
                },
                weight: pavis_core::Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .expect("upstream")
    }

    #[test]
    fn router_context_duration_conversion() {
        let core = pavis_core::Duration(NonZeroU32::new(100).unwrap());
        let std = core_duration_to_std(&core);
        assert_eq!(std.as_millis(), 100);
    }

    #[test]
    fn endpoint_label_formats_addresses() {
        let ip = EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: Port(NonZeroU16::new(8080).unwrap()),
        };
        assert_eq!(endpoint_label(&ip), "127.0.0.1:8080");

        let dns = EndpointAddr::Dns {
            host: Hostname("example.com".into()),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        assert_eq!(endpoint_label(&dns), "example.com:443");
    }

    #[test]
    fn scheduler_skips_disabled_health_checks() {
        let upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        let manager = Manager::new(&[upstream]).expect("manager");
        let scheduler = Scheduler::from_manager(&manager);
        assert!(scheduler.plans.is_empty());
    }

    #[test]
    fn scheduler_honors_intervals() {
        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(50).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(50).unwrap()),
        };
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let manager = Manager::new(&[upstream]).expect("manager");
        let mut scheduler = Scheduler::from_manager(&manager);
        let now = Instant::now() + Duration::from_millis(100);
        assert_eq!(scheduler.next_jobs(now).len(), 1);
        assert_eq!(scheduler.next_jobs(now).len(), 0);
        // Account for jitter: interval=50ms, max_jitter=5ms, so check at +56ms to be safe
        assert_eq!(
            scheduler.next_jobs(now + Duration::from_millis(56)).len(),
            1
        );
    }

    #[test]
    fn build_client_handles_disabled_tls() {
        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(50).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(50).unwrap()),
        };
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let client =
            build_health_client(&upstream, Duration::from_millis(10), None, Arc::new(vec![]));
        assert!(client.is_ok());
    }
}
