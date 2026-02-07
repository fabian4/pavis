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

    #[test]
    fn jitter_duration_zero_base() {
        let jitter = jitter_duration(Duration::ZERO);
        assert_eq!(jitter, Duration::ZERO);
    }

    #[test]
    fn jitter_duration_very_small_base() {
        let jitter = jitter_duration(Duration::from_millis(5));
        // With base=5ms, jitter_ms = 5/10 = 0, should return ZERO
        assert_eq!(jitter, Duration::ZERO);
    }

    #[test]
    fn jitter_duration_normal_base() {
        let jitter = jitter_duration(Duration::from_millis(1000));
        // With base=1000ms, jitter_ms = 100/10 = 50ms (capped at 50)
        // Jitter should be in range [0, 50ms]
        assert!(jitter.as_millis() <= 50);
    }

    #[test]
    fn jitter_duration_large_base() {
        let jitter = jitter_duration(Duration::from_secs(10));
        // With base=10000ms, jitter_ms = 1000/10 = 100, but capped at 50
        // Jitter should be in range [0, 50ms]
        assert!(jitter.as_millis() <= 50);
    }

    #[test]
    fn plan_state_is_due_before_time() {
        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(1000).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(500).unwrap()),
        };
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let manager = Manager::new(&[upstream]).expect("manager");

        let start = Instant::now();
        let scheduler = Scheduler::from_manager(&manager);
        let state = scheduler.plans.get("test").unwrap();

        // Use a timestamp unequivocally before next_due (which is now + [0..50]ms)
        let before = start - Duration::from_millis(100);
        assert!(!state.is_due(before));
    }

    #[test]
    fn plan_state_is_due_after_time() {
        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(10).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(5).unwrap()),
        };
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let manager = Manager::new(&[upstream]).expect("manager");
        let scheduler = Scheduler::from_manager(&manager);
        let state = scheduler.plans.get("test").unwrap();
        let far_future = Instant::now() + Duration::from_secs(100);
        assert!(state.is_due(far_future));
    }

    #[test]
    fn health_check_host_disabled_tls_with_dns() {
        let health = ActiveHealthCheck::Disabled;
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let endpoint = EndpointAddr::Dns {
            host: Hostname("backend.example.com".into()),
            port: Port(NonZeroU16::new(8080).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, Some("backend.example.com".to_string()));
    }

    #[test]
    fn health_check_host_disabled_tls_with_ip() {
        let health = ActiveHealthCheck::Disabled;
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let endpoint = EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: Port(NonZeroU16::new(8080).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, None);
    }

    #[test]
    fn health_check_host_tls_with_canonical_sni() {
        let tls = TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Enabled {
                name: Hostname("canonical.example.com".into()),
            },
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let health = ActiveHealthCheck::Disabled;
        let mut upstream = make_upstream(tls, health);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Enabled {
                name: Hostname("canonical.example.com".into()),
            },
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let endpoint = EndpointAddr::Dns {
            host: Hostname("backend.example.com".into()),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, Some("canonical.example.com".to_string()));
    }

    #[test]
    fn health_check_host_tls_reuse_sni_name() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Name(Hostname("sni.example.com".into())),
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Enabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let endpoint = EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, Some("sni.example.com".to_string()));
    }

    #[test]
    fn health_check_host_tls_reuse_sni_auto_with_dns() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Enabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let endpoint = EndpointAddr::Dns {
            host: Hostname("backend.example.com".into()),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, Some("backend.example.com".to_string()));
    }

    #[test]
    fn health_check_host_tls_reuse_sni_auto_with_ip() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Enabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let endpoint = EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, None);
    }

    #[test]
    fn health_check_host_tls_reuse_sni_disabled() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::CaOnly,
            sni: SniName::Disabled,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Enabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let endpoint = EndpointAddr::Dns {
            host: Hostname("backend.example.com".into()),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, None);
    }

    #[test]
    fn health_check_host_tls_no_reuse_sni_name() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Name(Hostname("sni.example.com".into())),
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let endpoint = EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, Some("sni.example.com".to_string()));
    }

    #[test]
    fn health_check_host_tls_no_reuse_sni_auto_with_dns() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let endpoint = EndpointAddr::Dns {
            host: Hostname("backend.example.com".into()),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        let host = health_check_host(&upstream, &endpoint);
        assert_eq!(host, Some("backend.example.com".to_string()));
    }

    #[test]
    fn build_client_tls_verify_disabled() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::Disabled,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let client = build_health_client(
            &upstream,
            Duration::from_millis(100),
            None,
            Arc::new(vec![]),
        );
        assert!(client.is_ok());
    }

    #[test]
    fn build_client_tls_verify_ca_only() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::CaOnly,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let client = build_health_client(
            &upstream,
            Duration::from_millis(100),
            None,
            Arc::new(vec![]),
        );
        assert!(client.is_ok());
    }

    #[test]
    fn build_client_tls_verify_full() {
        let mut upstream = make_upstream(TlsPolicy::Disabled, ActiveHealthCheck::Disabled);
        upstream.tls = TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: pavis_core::ClientCert::Disabled,
            ca: pavis_core::UpstreamCa::System,
        };
        let client = build_health_client(
            &upstream,
            Duration::from_millis(100),
            None,
            Arc::new(vec![]),
        );
        assert!(client.is_ok());
    }

    #[test]
    fn upstream_health_monitor_service_name() {
        let validated = pavis_core::validate_runtime(
            pavis_core::RuntimeConfigBuilder::new()
                .telemetry(pavis_core::Telemetry {
                    level: pavis_core::LogLevel::Info,
                    pingora: pavis_core::LogLevel::Warn,
                    service_name: pavis_core::ServiceName("test".to_string()),
                    metrics: pavis_core::Metrics::Disabled,
                    access_log: pavis_core::AccessLogPolicy::Disabled,
                    tracing: pavis_core::TracingPolicy::Disabled,
                })
                .shutdown(pavis_core::ShutdownPolicy::Disabled)
                .admin(pavis_core::AdminConfig::Disabled)
                .add_listener(
                    pavis_core::ListenerBuilder::new()
                        .name(pavis_core::ListenerName("test".to_string()))
                        .address(std::net::SocketAddr::new(
                            IpAddr::V4(Ipv4Addr::LOCALHOST),
                            8080,
                        ))
                        .workers(pavis_core::WorkerCount::Auto)
                        .tls(pavis_core::TlsConfig::Disabled)
                        .build()
                        .unwrap(),
                )
                .build()
                .unwrap(),
        )
        .unwrap();
        let state = crate::state::RuntimeState::from_config(&validated).unwrap();
        let monitor = UpstreamHealthMonitor::new(Arc::new(RuntimeStateHandle::new(state)));
        assert_eq!(monitor.name(), "upstream_health_monitor");
    }

    #[tokio::test]
    async fn test_probe_job_run_empty_endpoints() {
        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(50).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(50).unwrap()),
        };
        let mut u = make_upstream(TlsPolicy::Disabled, health);
        u.endpoints = vec![];
        let cluster = Arc::new(Cluster::new(u));
        let plan = Arc::new(HealthProbePlan::build("test", &cluster).unwrap().unwrap());

        let job = ProbeJob::new(plan, cluster);
        job.run().await; // Should return early without error
    }

    #[test]
    fn test_mark_all_unhealthy() {
        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
        };
        let u = make_upstream(TlsPolicy::Disabled, health);
        let endpoints = u.endpoints.clone();
        let cluster = Cluster::new(u);
        mark_all_unhealthy(&cluster, &endpoints);
        // Should have no eligible endpoints now
        assert!(cluster.select_endpoint().is_none());
    }

    #[test]
    fn test_jitter_distribution() {
        let base = Duration::from_millis(100);
        let mut zero = false;
        let mut positive = false;
        for _ in 0..100 {
            let j = jitter_duration(base);
            assert!(j.as_millis() <= 10);
            if j.as_millis() == 0 {
                zero = true;
            }
            if j.as_millis() > 0 {
                positive = true;
            }
        }
        assert!(zero || positive);
    }

    #[tokio::test]
    async fn test_health_probe_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        });

        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
        };
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let cluster = Arc::new(Cluster::new(upstream));
        let plan = HealthProbePlan::build("test", &cluster).unwrap().unwrap();

        let endpoint = Endpoint {
            address: EndpointAddr::Ip {
                address: addr.ip(),
                port: Port(NonZeroU16::new(addr.port()).unwrap()),
            },
            weight: pavis_core::Weight(NonZeroU16::new(1).unwrap()),
        };

        let result = plan.probe(&cluster, &endpoint).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_health_probe_failure() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
            let response = "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n";
            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        });

        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
        };
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let cluster = Arc::new(Cluster::new(upstream));
        let plan = HealthProbePlan::build("test", &cluster).unwrap().unwrap();

        let endpoint = Endpoint {
            address: EndpointAddr::Ip {
                address: addr.ip(),
                port: Port(NonZeroU16::new(addr.port()).unwrap()),
            },
            weight: pavis_core::Weight(NonZeroU16::new(1).unwrap()),
        };

        let result = plan.probe(&cluster, &endpoint).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_probe_job_execution() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        });

        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
        };
        let mut upstream = make_upstream(TlsPolicy::Disabled, health);
        upstream.endpoints = vec![Endpoint {
            address: EndpointAddr::Ip {
                address: addr.ip(),
                port: Port(NonZeroU16::new(addr.port()).unwrap()),
            },
            weight: pavis_core::Weight(NonZeroU16::new(1).unwrap()),
        }];

        let cluster = Arc::new(Cluster::new(upstream));
        let plan = Arc::new(HealthProbePlan::build("test", &cluster).unwrap().unwrap());

        let job = ProbeJob::new(plan, cluster.clone());
        job.run().await;

        // Verify endpoint is healthy
        let endpoints = cluster.current_endpoints();
        assert_eq!(endpoints.len(), 1);
        assert!(cluster.select_endpoint().is_some());
    }

    #[tokio::test]
    async fn test_probe_job_mark_unhealthy() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await;
            let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        });

        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
        };
        let mut upstream = make_upstream(TlsPolicy::Disabled, health);
        upstream.endpoints = vec![Endpoint {
            address: EndpointAddr::Ip {
                address: addr.ip(),
                port: Port(NonZeroU16::new(addr.port()).unwrap()),
            },
            weight: pavis_core::Weight(NonZeroU16::new(1).unwrap()),
        }];

        let cluster = Arc::new(Cluster::new(upstream));
        let plan = Arc::new(HealthProbePlan::build("test", &cluster).unwrap().unwrap());

        let job = ProbeJob::new(plan, cluster.clone());
        job.run().await;

        // Verify endpoint is unhealthy (filtered out by select_endpoint)
        assert!(cluster.select_endpoint().is_none());
    }

    #[tokio::test]
    async fn test_probe_connection_refused() {
        // Bind to get a free port, then drop listener so connection is refused
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(100).unwrap()),
        };
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let cluster = Arc::new(Cluster::new(upstream));
        let plan = HealthProbePlan::build("test", &cluster).unwrap().unwrap();

        let endpoint = Endpoint {
            address: EndpointAddr::Ip {
                address: addr.ip(),
                port: Port(NonZeroU16::new(addr.port()).unwrap()),
            },
            weight: pavis_core::Weight(NonZeroU16::new(1).unwrap()),
        };

        let result = plan.probe(&cluster, &endpoint).await;
        // Should error (connection refused)
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_upstream_health_monitor_loop() {
        let health = ActiveHealthCheck::Enabled {
            path: pavis_core::Path("/health".into()),
            interval: pavis_core::Duration(NonZeroU32::new(10).unwrap()),
            timeout: pavis_core::Duration(NonZeroU32::new(10).unwrap()),
        };
        let upstream = make_upstream(TlsPolicy::Disabled, health);
        let config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(pavis_core::Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Warn,
                service_name: pavis_core::ServiceName("test".to_string()),
                metrics: pavis_core::Metrics::Disabled,
                access_log: pavis_core::AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(
                pavis_core::ListenerBuilder::new()
                    .name(pavis_core::ListenerName("test".to_string()))
                    .address(std::net::SocketAddr::new(
                        IpAddr::V4(Ipv4Addr::LOCALHOST),
                        8080,
                    ))
                    .build()
                    .unwrap(),
            )
            .add_upstream(upstream)
            .build()
            .unwrap();
        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
        let state = crate::state::RuntimeState::from_config(&validated).unwrap();
        let handle = Arc::new(RuntimeStateHandle::new(state));
        let mut monitor = UpstreamHealthMonitor::new(handle.clone());

        let (tx, rx) = watch::channel(false);
        let monitor_handle = tokio::spawn(async move {
            monitor.start_service(None, rx, 1).await;
        });

        // Let it run for a bit
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Update state to trigger scheduler refresh
        let mut new_config = validated.clone().into_inner();
        new_config.upstreams[0].name = UpstreamName("new".to_string());
        let new_validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(new_config) };
        let new_state = crate::state::RuntimeState::from_config(&new_validated).unwrap();
        handle.store(new_state);

        tokio::time::sleep(Duration::from_millis(50)).await;

        tx.send(true).unwrap();
        monitor_handle.await.unwrap();
    }
}
