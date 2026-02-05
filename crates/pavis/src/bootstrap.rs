use crate::admin;
use crate::agent::ConfigAgent;
use crate::listener::tls::TlsRuntime;
use crate::proxy::Proxy;
use crate::state::{RuntimeState, RuntimeStateHandle};
use crate::telemetry::Telemetry;
use crate::telemetry::tracing::{ReloadHandle, maybe_init_tracing};
use crate::upstream::{UpstreamHealthMonitor, UpstreamResolver};
use anyhow::{Context, Result, bail};
use pavis_core::{AccessLogPolicy, ShutdownPolicy, ValidatedRuntimeConfig, WorkerCount};
use pingora::proxy::http_proxy_service;
use pingora::server::{Server, configuration::ServerConf};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct BootstrapOptions {
    pub config_path: PathBuf,
    pub relay_url: Option<String>,
}

pub struct BootstrapPlan {
    server: Server,
}

impl BootstrapPlan {
    pub fn build(
        config: Arc<ValidatedRuntimeConfig>,
        reload_handle: ReloadHandle,
        options: BootstrapOptions,
    ) -> Result<Self> {
        let BootstrapOptions {
            config_path,
            relay_url,
        } = options;

        if config.listeners.is_empty() {
            bail!("No listeners configured in runtime config.");
        }

        let access_log_desc = format_access_log(&config.telemetry.access_log);
        let max_threads = max_listener_threads(&config);
        tracing::info!(
            config = %config_path.display(),
            listener_count = config.listeners.len(),
            max_threads = ?max_threads,
            access_log = %access_log_desc,
            "Pavis starting"
        );
        tracing::info!("TLS backend: OpenSSL (only supported backend)");

        let server_conf = build_server_conf(&config);
        let mut server = Server::new_with_opt_and_conf(None, server_conf);
        server.bootstrap();
        let server_conf_arc = server.configuration.clone();

        let runtime_state = RuntimeState::from_config(&config)?;
        let state_handle = Arc::new(RuntimeStateHandle::new(runtime_state));

        let (telemetry, access_log_worker, metrics_worker, tracing_service) =
            Telemetry::new(&config.telemetry, Some(reload_handle.clone()));
        let telemetry = Arc::new(telemetry);

        let config_agent = relay_url
            .map(|relay| {
                build_config_agent(
                    relay,
                    config_path.clone(),
                    state_handle.clone(),
                    telemetry.clone(),
                    reload_handle.clone(),
                )
            })
            .transpose()?;

        let resolver = UpstreamResolver::new(state_handle.clone(), Duration::from_secs(10))
            .context(
                "failed to initialize upstream resolver (check DNS settings and PAVIS_DNS_SERVER)",
            )?;
        let health_monitor = UpstreamHealthMonitor::new(state_handle.clone());

        let tls_runtime = TlsRuntime::new();
        for listener in &config.listeners {
            let proxy_app = Proxy {
                state: state_handle.clone(),
                telemetry: telemetry.clone(),
            };

            let mut proxy_service = http_proxy_service(&server_conf_arc, proxy_app);
            let listen_addr_str = listener.address.to_string();

            if let Some(tls_settings) = tls_runtime.build(&listener.name, &listener.tls)? {
                proxy_service.add_tls_with_settings(&listen_addr_str, None, tls_settings);
            } else {
                proxy_service.add_tcp(&listen_addr_str);
            }

            tracing::info!(
                name = %listener.name.0,
                addr = %listen_addr_str,
                "Listener registered"
            );
            server.add_service(proxy_service);
        }

        server.add_service(access_log_worker);
        server.add_service(resolver);
        server.add_service(health_monitor);
        server.add_service(tracing_service);
        if let Some(metrics_worker) = metrics_worker {
            server.add_service(metrics_worker);
        }

        let admin_worker = admin::AdminApiWorker::new(config.admin, state_handle.clone());
        server.add_service(admin_worker);

        if let Some(agent) = config_agent {
            server.add_service(agent.worker());
        }

        Ok(Self { server })
    }

    pub fn run(self) -> Result<()> {
        tracing::info!("Pavis initialization complete, starting server");
        self.server.run_forever()
    }
}

fn format_access_log(policy: &AccessLogPolicy) -> String {
    match policy {
        AccessLogPolicy::Disabled => "off".to_string(),
        AccessLogPolicy::Stdout => "stdout".to_string(),
        AccessLogPolicy::File(path) => format!("file:{}", path.0),
        #[allow(unreachable_patterns)]
        &_ => "off".to_string(),
    }
}

fn build_server_conf(config: &ValidatedRuntimeConfig) -> ServerConf {
    let mut server_conf = ServerConf {
        daemon: false,
        ..Default::default()
    };
    server_conf.max_retries = u16::MAX as usize;
    if let Some(threads) = max_listener_threads(config) {
        server_conf.threads = threads as usize;
    }

    match config.shutdown {
        ShutdownPolicy::Disabled => {
            server_conf.grace_period_seconds = Some(0);
        }
        ShutdownPolicy::Enabled { drain_timeout } => {
            let timeout_ms = drain_timeout.0.get();
            let timeout_secs = (timeout_ms / 1000).max(1);
            server_conf.grace_period_seconds = Some(timeout_secs as u64);
            tracing::debug!(
                grace_period_seconds = timeout_secs,
                drain_timeout_ms = timeout_ms,
                "Configured graceful shutdown with drain timeout"
            );
        }
        #[allow(unreachable_patterns)]
        _ => {}
    }

    server_conf
}

fn max_listener_threads(config: &ValidatedRuntimeConfig) -> Option<u64> {
    config
        .listeners
        .iter()
        .filter_map(|l| match l.workers {
            WorkerCount::Count(count) => Some(count.get() as u64),
            WorkerCount::Auto => None,
            #[allow(unreachable_patterns)]
            _ => None,
        })
        .max()
}

fn build_config_agent(
    relay_url: String,
    config_path: PathBuf,
    state_handle: Arc<RuntimeStateHandle>,
    telemetry: Arc<Telemetry>,
    reload_handle: ReloadHandle,
) -> Result<Arc<ConfigAgent>> {
    let agent = ConfigAgent::new(
        relay_url,
        config_path,
        state_handle,
        Duration::from_secs(60),
    )?;

    let tracing_slot = telemetry.tracing.clone();
    let tracing_metrics = telemetry.metrics.clone();
    agent.on_update(move |config| {
        maybe_init_tracing(
            &config.telemetry.tracing,
            &config.telemetry.service_name.0,
            Some(&reload_handle),
            &tracing_slot,
            tracing_metrics.clone(),
        );
    });

    if let Some(metrics) = telemetry.metrics.as_ref() {
        agent.set_metrics_handle(metrics.clone());
    }

    Ok(Arc::new(agent))
}

#[cfg(test)]
mod tests {
    use super::{BootstrapOptions, BootstrapPlan, format_access_log, max_listener_threads};
    use crate::telemetry::tracing::{ReloadHandle, ReloadableLayer};
    use pavis_core::{
        AccessLogPolicy, AdminConfig, ListenerBuilder, ListenerName, LogLevel, Metrics, Path,
        RuntimeConfigBuilder, ServiceName, Telemetry as RuntimeTelemetry, TlsConfig, WorkerCount,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::NonZeroU16;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn sample_config() -> pavis_core::ValidatedRuntimeConfig {
        let listener = ListenerBuilder::new()
            .name(ListenerName("listener".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .workers(WorkerCount::Count(NonZeroU16::new(1).unwrap()))
            .tls(TlsConfig::Disabled)
            .build()
            .expect("listener");

        let telemetry = RuntimeTelemetry {
            level: LogLevel::Info,
            pingora: LogLevel::Info,
            service_name: ServiceName("svc".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        };

        let config = RuntimeConfigBuilder::new()
            .telemetry(telemetry)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(listener)
            .build()
            .expect("config");

        pavis_core::validate_runtime(config).expect("validated")
    }

    #[test]
    fn bootstrap_plan_builds_without_relay() {
        let config = Arc::new(sample_config());
        let reload_handle: ReloadHandle = ReloadableLayer::new();
        let options = BootstrapOptions {
            config_path: PathBuf::from("/tmp/config.pvs"),
            relay_url: None,
        };

        let plan = BootstrapPlan::build(config, reload_handle, options).expect("plan builds");
        drop(plan);
    }

    #[test]
    fn format_access_log_covers_variants() {
        assert_eq!(format_access_log(&AccessLogPolicy::Disabled), "off");
        assert_eq!(format_access_log(&AccessLogPolicy::Stdout), "stdout");
        let file = AccessLogPolicy::File(Path("/tmp/test".to_string()));
        assert_eq!(format_access_log(&file), "file:/tmp/test");
    }

    #[test]
    fn max_listener_threads_prefers_highest_count() {
        let listener_auto = ListenerBuilder::new()
            .name(ListenerName("auto".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 30080))
            .workers(WorkerCount::Auto)
            .tls(TlsConfig::Disabled)
            .build()
            .expect("auto");

        let listener_count = ListenerBuilder::new()
            .name(ListenerName("count".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 30081))
            .workers(WorkerCount::Count(NonZeroU16::new(4).unwrap()))
            .tls(TlsConfig::Disabled)
            .build()
            .expect("count");

        let telemetry = RuntimeTelemetry {
            level: LogLevel::Info,
            pingora: LogLevel::Info,
            service_name: ServiceName("svc".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        };

        let config = RuntimeConfigBuilder::new()
            .telemetry(telemetry)
            .admin(AdminConfig::Disabled)
            .add_listener(listener_auto)
            .add_listener(listener_count)
            .build()
            .expect("config");
        let validated = pavis_core::validate_runtime(config).expect("validated");

        assert_eq!(max_listener_threads(&validated), Some(4));
    }

    #[test]
    fn test_bootstrap_plan_build_fail_no_listeners() {
        let listener = ListenerBuilder::new()
            .name(ListenerName("test".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
            .build()
            .unwrap();
        let telemetry = RuntimeTelemetry {
            level: LogLevel::Info,
            pingora: LogLevel::Info,
            service_name: ServiceName("svc".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        };
        let mut config = RuntimeConfigBuilder::new()
            .telemetry(telemetry)
            .add_listener(listener)
            .build()
            .unwrap();
        // Clear listeners to trigger the error
        config.listeners.clear();

        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
        let reload_handle: ReloadHandle = ReloadableLayer::new();
        let options = BootstrapOptions {
            config_path: PathBuf::from("/tmp/config.pvs"),
            relay_url: None,
        };

        let res = BootstrapPlan::build(Arc::new(validated), reload_handle, options);
        assert!(res.is_err());
        let err_msg = format!("{}", res.err().unwrap());
        assert!(err_msg.contains("No listeners configured"));
    }

    #[test]
    fn test_build_server_conf_graceful_shutdown() {
        let config = sample_config();
        let mut inner = config.as_ref().clone();
        inner.shutdown = pavis_core::ShutdownPolicy::Enabled {
            drain_timeout: pavis_core::Duration(std::num::NonZeroU32::new(5000).unwrap()),
        };
        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(inner) };

        let server_conf = super::build_server_conf(&validated);
        assert_eq!(server_conf.grace_period_seconds, Some(5));
    }

    #[test]
    fn test_bootstrap_plan_build_with_relay() {
        let config = Arc::new(sample_config());
        let reload_handle: ReloadHandle = ReloadableLayer::new();
        let options = BootstrapOptions {
            config_path: PathBuf::from("/tmp/config.pvs"),
            relay_url: Some("http://localhost:8080".to_string()),
        };

        let plan = BootstrapPlan::build(config, reload_handle, options).expect("plan builds");
        drop(plan);
    }
}
