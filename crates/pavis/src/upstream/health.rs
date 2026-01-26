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

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        ClientCert, Discovery, EndpointAddr, Hostname, HttpVersion, LoadBalancer, Pool, Port,
        SniName, TlsPolicy, TlsVerify, Upstream, UpstreamBuilder, UpstreamCa, UpstreamId,
        UpstreamName,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::{NonZeroU16, NonZeroU32};

    fn make_upstream(tls: TlsPolicy) -> Upstream {
        UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("test".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .pool(Pool::default())
            .tls(tls)
            .build()
            .expect("failed to build upstream")
    }

    #[test]
    fn test_core_duration_to_std() {
        let core = pavis_core::Duration(NonZeroU32::new(100).unwrap());
        let std = core_duration_to_std(&core);
        assert_eq!(std.as_millis(), 100);
    }

    #[test]
    fn test_endpoint_label() {
        let ip = EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: Port(NonZeroU16::new(8080).unwrap()),
        };
        assert_eq!(endpoint_label(&ip), "127.0.0.1:8080");

        let dns = EndpointAddr::Dns {
            host: Hostname("example.com".to_string()),
            port: Port(NonZeroU16::new(443).unwrap()),
        };
        assert_eq!(endpoint_label(&dns), "example.com:443");
    }

    #[test]
    fn test_health_check_host() {
        let ip_endpoint = EndpointAddr::Ip {
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: Port(NonZeroU16::new(8080).unwrap()),
        };
        let dns_endpoint = EndpointAddr::Dns {
            host: Hostname("example.com".to_string()),
            port: Port(NonZeroU16::new(443).unwrap()),
        };

        // Case 1: TLS Disabled
        let u1 = make_upstream(TlsPolicy::Disabled);
        assert_eq!(health_check_host(&u1, &ip_endpoint), None);
        assert_eq!(
            health_check_host(&u1, &dns_endpoint),
            Some("example.com".to_string())
        );

        // Case 2: TLS Enabled, SNI Disabled
        let u2 = make_upstream(TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Disabled,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: ClientCert::Disabled,
            ca: UpstreamCa::System,
        });
        assert_eq!(health_check_host(&u2, &ip_endpoint), None);
        assert_eq!(health_check_host(&u2, &dns_endpoint), None);

        // Case 3: TLS Enabled, SNI Auto
        let u3 = make_upstream(TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Auto,
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: ClientCert::Disabled,
            ca: UpstreamCa::System,
        });
        assert_eq!(health_check_host(&u3, &ip_endpoint), None);
        assert_eq!(
            health_check_host(&u3, &dns_endpoint),
            Some("example.com".to_string())
        );

        // Case 4: TLS Enabled, SNI Explicit
        let u4 = make_upstream(TlsPolicy::Enabled {
            verify: TlsVerify::Full,
            sni: SniName::Name(Hostname("custom.host".to_string())),
            canonical_sni: pavis_core::CanonicalSni::Disabled,
            reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
            cert: ClientCert::Disabled,
            ca: UpstreamCa::System,
        });
        assert_eq!(
            health_check_host(&u4, &ip_endpoint),
            Some("custom.host".to_string())
        );
        assert_eq!(
            health_check_host(&u4, &dns_endpoint),
            Some("custom.host".to_string())
        );
    }
}

#[cfg(test)]
mod health_monitor_extra {
    use super::*;
    use crate::upstream::Cluster;
    use axum::http::StatusCode;
    use axum::{Router, routing::get, serve};
    use pavis_core::{
        ActiveHealthCheck, ClientCert, ClientCertChain, Discovery, Endpoint, EndpointAddr,
        HttpVersion, LoadBalancer, Path as CorePath, Pool, Port, SniName, TlsPolicy, TlsVerify,
        Upstream, UpstreamBuilder, UpstreamCa, UpstreamId, UpstreamName, Weight,
    };
    use rcgen::{CertifiedKey, generate_simple_self_signed};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::{NonZeroU16, NonZeroU32};
    use std::path::PathBuf;
    use std::time::Duration as StdDuration;

    struct TempFile {
        path: PathBuf,
    }

    impl TempFile {
        fn new(prefix: &str, contents: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!("{prefix}-{}.pem", rand::random::<u64>()));
            std::fs::write(&path, contents).expect("write temp pem");
            Self { path }
        }

        fn to_path(&self) -> pavis_core::Path {
            pavis_core::Path(self.path.to_string_lossy().to_string())
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    async fn spawn_test_server(status: StatusCode) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route("/health", get(move || async move { status }));
        tokio::spawn(async move {
            serve(listener, app).await.expect("serve test app");
        });
        addr
    }

    fn health_checked_upstream() -> Upstream {
        UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(7).unwrap()))
            .name(UpstreamName("hc".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .pool(Pool::default())
            .health_check(ActiveHealthCheck::Enabled {
                path: CorePath("/health".to_string()),
                interval: pavis_core::Duration(NonZeroU32::new(500).unwrap()),
                timeout: pavis_core::Duration(NonZeroU32::new(500).unwrap()),
            })
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .expect("upstream")
    }

    #[tokio::test]
    async fn probe_endpoint_reports_success() {
        let addr = spawn_test_server(StatusCode::OK).await;
        let mut upstream = health_checked_upstream();
        if let Some(ep) = upstream.endpoints.get_mut(0) {
            ep.address = EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(addr.port()).unwrap()),
            };
        }

        let client = build_health_client(&upstream, StdDuration::from_secs(1)).unwrap();
        let endpoint = upstream.endpoints.first().cloned().unwrap();
        assert!(
            probe_endpoint(&client, &upstream, &endpoint, "/health")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn probe_endpoint_reports_failure_for_bad_status() {
        let addr = spawn_test_server(StatusCode::INTERNAL_SERVER_ERROR).await;
        let mut upstream = health_checked_upstream();
        if let Some(ep) = upstream.endpoints.get_mut(0) {
            ep.address = EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(addr.port()).unwrap()),
            };
        }

        let client = build_health_client(&upstream, StdDuration::from_secs(1)).unwrap();
        let endpoint = upstream.endpoints.first().cloned().unwrap();
        let healthy = probe_endpoint(&client, &upstream, &endpoint, "/health")
            .await
            .unwrap();
        assert!(!healthy);
    }

    #[tokio::test]
    async fn probe_endpoint_handles_network_errors() {
        let unreachable = Endpoint {
            address: EndpointAddr::Ip {
                address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: Port(NonZeroU16::new(65_000).unwrap()),
            },
            weight: Weight(NonZeroU16::new(1).unwrap()),
        };
        let upstream = health_checked_upstream();
        let client = build_health_client(&upstream, StdDuration::from_millis(100)).unwrap();
        let result = probe_endpoint(&client, &upstream, &unreachable, "/health").await;
        assert!(result.is_err());
    }

    #[test]
    fn build_health_client_rejects_missing_ca() {
        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(3).unwrap()))
            .name(UpstreamName("missing-ca".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .pool(Pool::default())
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::Full,
                sni: SniName::Disabled,
                canonical_sni: pavis_core::CanonicalSni::Disabled,
                reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
                cert: ClientCert::Disabled,
                ca: UpstreamCa::File {
                    path: pavis_core::Path("/nonexistent/ca.pem".into()),
                },
            })
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        let err = build_health_client(&upstream, StdDuration::from_secs(1)).unwrap_err();
        assert!(err.to_string().contains("failed to read CA bundle"));
    }

    #[test]
    fn build_health_client_supports_client_identity() {
        let CertifiedKey { cert, key_pair } =
            generate_simple_self_signed(vec!["client.test".to_string()]).unwrap();
        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();
        let chain_cert = generate_simple_self_signed(vec!["chain.test".to_string()]).unwrap();
        let chain_pem = chain_cert.cert.pem();

        let cert_file = TempFile::new("client-cert", cert_pem.as_bytes());
        let key_file = TempFile::new("client-key", key_pem.as_bytes());
        let chain_file = TempFile::new("client-chain", chain_pem.as_bytes());

        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(4).unwrap()))
            .name(UpstreamName("mtls".to_string()))
            .discovery(Discovery::Static)
            .balancer(LoadBalancer::Random)
            .protocol(HttpVersion::H1)
            .pool(Pool::default())
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::Disabled,
                sni: SniName::Auto,
                canonical_sni: pavis_core::CanonicalSni::Disabled,
                reuse_across_sni: pavis_core::ReuseAcrossSni::Disabled,
                cert: ClientCert::Enabled {
                    cert_path: cert_file.to_path(),
                    key_path: key_file.to_path(),
                    chain: ClientCertChain::File {
                        path: chain_file.to_path(),
                    },
                },
                ca: UpstreamCa::System,
            })
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port: Port(NonZeroU16::new(80).unwrap()),
                },
                weight: Weight(NonZeroU16::new(1).unwrap()),
            })
            .build()
            .unwrap();

        build_health_client(&upstream, StdDuration::from_secs(1)).expect("client");
    }

    #[test]
    fn mark_all_unhealthy_clears_selection() {
        let upstream = health_checked_upstream();
        let cluster = Cluster::new(upstream);
        let endpoints = cluster.current_endpoints();
        assert!(!endpoints.is_empty());
        assert!(cluster.select_endpoint().is_some());
        mark_all_unhealthy(&cluster, &endpoints);
        assert!(cluster.select_endpoint().is_none());
    }
}
