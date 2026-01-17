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
        let mut last_checks: HashMap<String, Instant> = HashMap::new();
        let mut clients: HashMap<String, Client> = HashMap::new();
        let mut tick = tokio::time::interval(Duration::from_millis(200));

        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tick.tick() => {
                    let state = self.state.load();
                    let state_ptr = Arc::as_ptr(&state) as usize;
                    if state_ptr != last_state_ptr {
                        last_state_ptr = state_ptr;
                        last_checks.clear();
                        clients.clear();
                    }

                    for (name, cluster) in state.upstream_manager.iter() {
                        let (path, interval, timeout) = match &cluster.config.health_check {
                            ActiveHealthCheck::Enabled { path, interval, timeout } => (
                                path.0.clone(),
                                core_duration_to_std(interval),
                                core_duration_to_std(timeout),
                            ),
                            ActiveHealthCheck::Disabled => continue,
                            #[allow(unreachable_patterns)]
                            _ => continue,
                        };

                        let now = Instant::now();
                        let should_run = match last_checks.get(name.as_str()) {
                            Some(last) => now.duration_since(*last) >= interval,
                            None => true,
                        };
                        if !should_run {
                            continue;
                        }
                        last_checks.insert(name.clone(), now);

                        let client = match clients.get(name.as_str()) {
                            Some(client) => client.clone(),
                            None => match build_health_client(&cluster.config, timeout) {
                                Ok(client) => {
                                    clients.insert(name.clone(), client.clone());
                                    client
                                }
                                Err(err) => {
                                    tracing::warn!(
                                        upstream = %name,
                                        error = %err,
                                        "failed to build health check client"
                                    );
                                    mark_all_unhealthy(cluster, &cluster.current_endpoints());
                                    continue;
                                }
                            },
                        };

                        let endpoints = cluster.current_endpoints();
                        for endpoint in endpoints {
                            let healthy = match probe_endpoint(&client, &cluster.config, &endpoint, &path).await {
                                Ok(healthy) => healthy,
                                Err(err) => {
                                    tracing::debug!(
                                        upstream = %name,
                                        endpoint = %endpoint_label(&endpoint.address),
                                        error = %err,
                                        "health probe failed"
                                    );
                                    false
                                }
                            };
                            cluster.set_active_health(&endpoint.address, healthy);
                        }
                    }
                }
            }
        }
    }

    fn name(&self) -> &str {
        "upstream_health_monitor"
    }
}

fn mark_all_unhealthy(cluster: &crate::upstream::Cluster, endpoints: &[Endpoint]) {
    for endpoint in endpoints {
        cluster.set_active_health(&endpoint.address, false);
    }
}

fn core_duration_to_std(duration: &pavis_core::Duration) -> Duration {
    Duration::from_millis(duration.0.get() as u64)
}

fn endpoint_label(addr: &EndpointAddr) -> String {
    match addr {
        EndpointAddr::Ip { address, port } => format!("{}:{}", address, port.0.get()),
        EndpointAddr::Dns { host, port } => format!("{}:{}", host.0, port.0.get()),
        #[allow(unreachable_patterns)]
        _ => "unknown".to_string(),
    }
}

async fn probe_endpoint(
    client: &Client,
    upstream: &pavis_core::Upstream,
    endpoint: &Endpoint,
    path: &str,
) -> Result<bool> {
    let scheme = match upstream.tls {
        TlsPolicy::Disabled => "http",
        TlsPolicy::Enabled { .. } => "https",
        #[allow(unreachable_patterns)]
        _ => "http",
    };

    let (host, port) = match &endpoint.address {
        EndpointAddr::Ip { address, port } => (address.to_string(), port.0.get()),
        EndpointAddr::Dns { host, port } => (host.0.clone(), port.0.get()),
        #[allow(unreachable_patterns)]
        _ => ("127.0.0.1".to_string(), 80),
    };

    let url = format!("{scheme}://{host}:{port}{path}");
    let mut request = client.get(url);

    if let Some(host_header) = health_check_host(upstream, &endpoint.address) {
        request = request.header(reqwest::header::HOST, host_header);
    }

    let response = request
        .send()
        .await
        .context("health probe request failed")?;
    let status = response.status();
    Ok(status.is_success())
}

fn health_check_host(upstream: &pavis_core::Upstream, endpoint: &EndpointAddr) -> Option<String> {
    match &upstream.tls {
        TlsPolicy::Enabled { sni, .. } => match sni {
            SniName::Name(name) => Some(name.0.clone()),
            SniName::Auto => match endpoint {
                EndpointAddr::Dns { host, .. } => Some(host.0.clone()),
                _ => None,
            },
            SniName::Disabled => None,
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

fn build_health_client(upstream: &pavis_core::Upstream, timeout: Duration) -> Result<Client> {
    let mut builder = Client::builder().timeout(timeout).connect_timeout(timeout);

    if let TlsPolicy::Enabled {
        verify, ca, cert, ..
    } = &upstream.tls
    {
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

        if let pavis_core::UpstreamCa::File { path } = ca {
            let pem = std::fs::read(&path.0)
                .with_context(|| format!("failed to read CA bundle {}", path.0))?;
            let cert = reqwest::Certificate::from_pem(&pem)
                .context("failed to parse CA bundle for health checks")?;
            builder = builder.add_root_certificate(cert);
        }

        if let pavis_core::ClientCert::Enabled {
            cert_path,
            key_path,
            chain,
        } = cert
        {
            let mut pem = std::fs::read(&cert_path.0)
                .with_context(|| format!("failed to read client cert {}", cert_path.0))?;
            if let pavis_core::ClientCertChain::File { path } = chain {
                let chain_pem = std::fs::read(&path.0)
                    .with_context(|| format!("failed to read client cert chain {}", path.0))?;
                pem.extend_from_slice(&chain_pem);
            }
            let key_pem = std::fs::read(&key_path.0)
                .with_context(|| format!("failed to read client key {}", key_path.0))?;
            pem.extend_from_slice(&key_pem);
            let identity = reqwest::Identity::from_pem(&pem)
                .context("failed to parse client identity for health checks")?;
            builder = builder.identity(identity);
        }
    }

    builder
        .build()
        .context("failed to build health check client")
}
