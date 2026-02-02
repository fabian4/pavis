use pavis_core::{
    AdminConfig, ClientAuth, ClientCert, ClientCertChain, Metrics, TlsConfig, TlsPolicy,
    UpstreamCa, ValidatedRuntimeConfig,
};
use std::collections::HashSet;
use std::fs::File;
use std::net::SocketAddr;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeEnvError {
    #[error("{context}: missing or unreadable file '{path}'")]
    MissingFile { context: String, path: String },
    #[error("{context}: port unavailable at {addr}")]
    PortUnavailable { context: String, addr: SocketAddr },
}

pub fn validate_runtime_env(
    config: &ValidatedRuntimeConfig,
    current: Option<&ValidatedRuntimeConfig>,
) -> Result<(), RuntimeEnvError> {
    validate_tls_files(config)?;
    validate_upstream_tls_files(config)?;
    validate_ports(config, current)?;
    Ok(())
}

fn validate_ports(
    config: &ValidatedRuntimeConfig,
    current: Option<&ValidatedRuntimeConfig>,
) -> Result<(), RuntimeEnvError> {
    let current_ports = current.map(collect_ports).unwrap_or_default();

    for listener in &config.listeners {
        let addr = listener.address;
        if current_ports.contains(&addr.port()) {
            continue;
        }
        preflight_bind(addr).map_err(|_| RuntimeEnvError::PortUnavailable {
            context: format!("listener '{}'", listener.name.0),
            addr,
        })?;
    }

    match config.admin {
        AdminConfig::Enabled { addr } if !current_ports.contains(&addr.port()) => {
            preflight_bind(addr).map_err(|_| RuntimeEnvError::PortUnavailable {
                context: "admin".to_string(),
                addr,
            })?;
        }
        _ => {}
    }

    match config.telemetry.metrics {
        Metrics::Enabled { addr } if !current_ports.contains(&addr.port()) => {
            preflight_bind(addr).map_err(|_| RuntimeEnvError::PortUnavailable {
                context: "metrics".to_string(),
                addr,
            })?;
        }
        _ => {}
    }

    Ok(())
}

fn collect_ports(config: &ValidatedRuntimeConfig) -> HashSet<u16> {
    let mut ports = HashSet::new();
    for listener in &config.listeners {
        ports.insert(listener.address.port());
    }
    if let AdminConfig::Enabled { addr } = config.admin {
        ports.insert(addr.port());
    }
    if let Metrics::Enabled { addr } = config.telemetry.metrics {
        ports.insert(addr.port());
    }
    ports
}

fn preflight_bind(addr: SocketAddr) -> std::io::Result<()> {
    let listener = std::net::TcpListener::bind(addr)?;
    drop(listener);
    Ok(())
}

fn validate_tls_files(config: &ValidatedRuntimeConfig) -> Result<(), RuntimeEnvError> {
    for listener in &config.listeners {
        if let TlsConfig::Enabled {
            cert_path,
            key_path,
            client_auth,
        } = &listener.tls
        {
            ensure_readable(
                &cert_path.0,
                format!("listener '{}' tls.cert_path", listener.name.0),
            )?;
            ensure_readable(
                &key_path.0,
                format!("listener '{}' tls.key_path", listener.name.0),
            )?;
            match client_auth {
                ClientAuth::Disabled => {}
                ClientAuth::Optional { ca_path } | ClientAuth::Required { ca_path } => {
                    ensure_readable(
                        &ca_path.0,
                        format!("listener '{}' tls.client_auth.ca_path", listener.name.0),
                    )?;
                }
                #[allow(unreachable_patterns)]
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_upstream_tls_files(config: &ValidatedRuntimeConfig) -> Result<(), RuntimeEnvError> {
    for upstream in &config.upstreams {
        if let TlsPolicy::Enabled { ca, cert, .. } = &upstream.tls {
            if let UpstreamCa::File { path } = ca {
                ensure_readable(
                    &path.0,
                    format!("upstream '{}' tls.ca.path", upstream.name.0),
                )?;
            }

            if let ClientCert::Enabled {
                cert_path,
                key_path,
                chain,
            } = cert
            {
                ensure_readable(
                    &cert_path.0,
                    format!("upstream '{}' tls.cert_path", upstream.name.0),
                )?;
                ensure_readable(
                    &key_path.0,
                    format!("upstream '{}' tls.key_path", upstream.name.0),
                )?;
                if let ClientCertChain::File { path } = chain {
                    ensure_readable(
                        &path.0,
                        format!("upstream '{}' tls.chain.path", upstream.name.0),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn ensure_readable(path: &str, context: String) -> Result<(), RuntimeEnvError> {
    File::open(path).map_err(|_| RuntimeEnvError::MissingFile {
        context,
        path: path.to_string(),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{
        AccessLogPolicy, AdminConfig, ClientAuth, ListenerBuilder, ListenerName, LogLevel, Metrics,
        RuntimeConfigBuilder, ServiceName, Telemetry, TlsConfig, TracingPolicy, WorkerCount,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn base_runtime() -> ValidatedRuntimeConfig {
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .workers(WorkerCount::Auto)
            .tls(TlsConfig::Disabled)
            .build()
            .expect("listener");

        let cfg = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("svc".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            })
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(AdminConfig::Disabled)
            .add_listener(listener)
            .build()
            .expect("config");
        pavis_core::validate_runtime(cfg).expect("validated")
    }

    #[test]
    fn validate_env_rejects_missing_listener_tls_files() {
        let mut cfg = base_runtime().into_inner();
        cfg.listeners[0].tls = TlsConfig::Enabled {
            cert_path: pavis_core::Path("missing-cert.pem".to_string()),
            key_path: pavis_core::Path("missing-key.pem".to_string()),
            client_auth: ClientAuth::Disabled,
        };
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(cfg) };
        let err = validate_runtime_env(&validated, None).expect_err("missing files");
        assert!(matches!(err, RuntimeEnvError::MissingFile { .. }));
    }

    #[test]
    fn validate_env_rejects_port_in_use() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();

        let mut cfg = base_runtime().into_inner();
        cfg.listeners[0].address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(cfg) };
        let err = validate_runtime_env(&validated, None).expect_err("port in use");
        assert!(matches!(err, RuntimeEnvError::PortUnavailable { .. }));
        drop(listener);
    }

    #[test]
    fn validate_env_rejects_admin_port_in_use() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();

        let mut cfg = base_runtime().into_inner();
        cfg.admin = AdminConfig::Enabled {
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        };
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(cfg) };
        let err = validate_runtime_env(&validated, None).expect_err("admin port in use");
        assert!(matches!(err, RuntimeEnvError::PortUnavailable { .. }));
        assert!(err.to_string().contains("admin"));
    }

    #[test]
    fn validate_env_rejects_metrics_port_in_use() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();

        let mut cfg = base_runtime().into_inner();
        cfg.telemetry.metrics = Metrics::Enabled {
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        };
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(cfg) };
        let err = validate_runtime_env(&validated, None).expect_err("metrics port in use");
        assert!(matches!(err, RuntimeEnvError::PortUnavailable { .. }));
        assert!(err.to_string().contains("metrics"));
    }

    #[test]
    fn validate_env_rejects_missing_client_auth_ca() {
        let mut cfg = base_runtime().into_inner();
        // Create a temporary file to pass cert/key checks
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_string_lossy().to_string();

        cfg.listeners[0].tls = TlsConfig::Enabled {
            cert_path: pavis_core::Path(path.clone()),
            key_path: pavis_core::Path(path),
            client_auth: ClientAuth::Required {
                ca_path: pavis_core::Path("missing-ca.pem".to_string()),
            },
        };
        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(cfg) };
        let err = validate_runtime_env(&validated, None).expect_err("missing ca");
        assert!(matches!(err, RuntimeEnvError::MissingFile { .. }));
        assert!(err.to_string().contains("tls.client_auth.ca_path"));
    }

    #[test]
    fn validate_env_rejects_missing_upstream_tls_files() {
        use pavis_core::{
            CanonicalSni, ConnectTimeout, ConnectionLimit, Endpoint, EndpointAddr, IdleTimeout,
            Pool, Port, ReuseAcrossSni, SniName, TlsPolicy, TlsVerify, UpstreamBuilder, UpstreamCa,
            UpstreamId, UpstreamName, Weight,
        };
        use std::num::{NonZeroU16, NonZeroU32};

        let upstream = UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(1).unwrap()))
            .name(UpstreamName("u".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(pavis_core::LoadBalancer::Random)
            .protocol(pavis_core::HttpVersion::H1)
            .pool(Pool {
                idle: IdleTimeout::Disabled,
                connect: ConnectTimeout::Disabled,
                max: ConnectionLimit(NonZeroU32::new(10).unwrap()),
                queue: Default::default(),
                tcp_keepalive: None,
                tcp_nodelay: None,
                recv_buffer_size: None,
            })
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::Disabled,
                sni: SniName::Auto,
                canonical_sni: CanonicalSni::Disabled,
                reuse_across_sni: ReuseAcrossSni::Disabled,
                cert: pavis_core::ClientCert::Disabled,
                ca: UpstreamCa::File {
                    path: pavis_core::Path("missing-ca.pem".to_string()),
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
            .expect("upstream");

        let mut cfg = base_runtime().into_inner();
        cfg.upstreams.push(upstream);

        let validated = unsafe { ValidatedRuntimeConfig::from_trusted(cfg) };
        let err = validate_runtime_env(&validated, None).expect_err("missing upstream ca");
        assert!(matches!(err, RuntimeEnvError::MissingFile { .. }));
        assert!(err.to_string().contains("tls.ca.path"));
    }
}
