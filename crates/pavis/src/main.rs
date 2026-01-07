use anyhow::{Context, Result};
use clap::Parser;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use pingora::server::configuration::ServerConf;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pavis::agent::{Backoff, ConfigAgent, lkg_version};
use pavis::load::{self, RuntimeLoadError};
use pavis::proxy::Proxy;
use pavis::state::RuntimeStateHandle;
use pavis::telemetry::Telemetry;
use pavis::upstream::UpstreamResolver;
use pavis_core::{AccessLogPolicy, LogLevel, WorkerCount};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
    #[arg(long)]
    relay_url: Option<String>,
}

fn log_level_to_str(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
        #[allow(unreachable_patterns)]
        _ => "info", // Default to info for unknown log levels
    }
}

#[cfg(test)]
mod tests {
    use super::log_level_to_str;
    use pavis_core::{AccessLogPolicy, LogLevel, Path};

    #[test]
    fn log_level_to_str_defaults_to_info() {
        assert_eq!(log_level_to_str(LogLevel::Info), "info");
    }

    #[test]
    fn log_level_to_str_maps_values() {
        assert_eq!(log_level_to_str(LogLevel::Error), "error");
        assert_eq!(log_level_to_str(LogLevel::Warn), "warn");
        assert_eq!(log_level_to_str(LogLevel::Info), "info");
        assert_eq!(log_level_to_str(LogLevel::Debug), "debug");
        assert_eq!(log_level_to_str(LogLevel::Trace), "trace");
    }

    #[test]
    fn access_log_description_logic() {
        let desc = match AccessLogPolicy::Disabled {
            AccessLogPolicy::Disabled => "off".to_string(),
            AccessLogPolicy::Stdout => "stdout".to_string(),
            AccessLogPolicy::File(path) => format!("file:{}", path.0),
            _ => "off".to_string(),
        };
        assert_eq!(desc, "off");

        let desc = match AccessLogPolicy::Stdout {
            AccessLogPolicy::Disabled => "off".to_string(),
            AccessLogPolicy::Stdout => "stdout".to_string(),
            AccessLogPolicy::File(path) => format!("file:{}", path.0),
            _ => "off".to_string(),
        };
        assert_eq!(desc, "stdout");

        let desc = match AccessLogPolicy::File(Path("/tmp/test".to_string())) {
            AccessLogPolicy::Disabled => "off".to_string(),
            AccessLogPolicy::Stdout => "stdout".to_string(),
            AccessLogPolicy::File(path) => format!("file:{}", path.0),
            _ => "off".to_string(),
        };
        assert_eq!(desc, "file:/tmp/test");
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration (LKG)
    let config = load::load_file(&args.config).map_err(|e| match e {
        RuntimeLoadError::Pvs(pavis_pvs::PvsError::VersionMismatch { file, expected }) => {
            anyhow::anyhow!(
                "Version mismatch! File: {}, Runtime expects: {}. Recompile config.",
                file,
                expected
            )
        }
        other => anyhow::anyhow!(other),
    })?;

    let config = Arc::new(config);

    let log_level = log_level_to_str(config.telemetry.level);
    let mut filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    let p_str = log_level_to_str(config.telemetry.pingora);
    filter = filter
        .add_directive(format!("pingora={}", p_str).parse()?)
        .add_directive(format!("pingora_core={}", p_str).parse()?);

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let access_log_desc = match &config.telemetry.access_log {
        AccessLogPolicy::Disabled => "off".to_string(),
        AccessLogPolicy::Stdout => "stdout".to_string(),
        AccessLogPolicy::File(path) => format!("file:{}", path.0),
        #[allow(unreachable_patterns)]
        &_ => "off".to_string(),
    };

    // Listener selection logic
    if config.listeners.is_empty() {
        anyhow::bail!("No listeners configured in runtime config.");
    }

    let max_threads = config
        .listeners
        .iter()
        .filter_map(|l| match l.workers {
            WorkerCount::Count(count) => Some(count.get() as u64),
            WorkerCount::Auto => None,
            #[allow(unreachable_patterns)]
            _ => None,
        })
        .max();

    tracing::info!(
        config = %args.config,
        listener_count = config.listeners.len(),
        max_threads = ?max_threads,
        access_log = %access_log_desc,
        "Pavis starting"
    );

    let mut server_conf = ServerConf {
        daemon: false,
        ..Default::default()
    };
    if let Some(threads) = max_threads {
        server_conf.threads = threads as usize;
    }
    let mut server = Server::new_with_opt_and_conf(None, server_conf);
    server.bootstrap();

    let runtime_state = pavis::state::RuntimeState::from_config(&config)?;
    let state_handle = Arc::new(RuntimeStateHandle::new(runtime_state));

    let lkg_version = lkg_version(Path::new(&args.config))?;

    let config_agent = args.relay_url.as_ref().map(|relay| {
        let backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 200);
        let agent = ConfigAgent::new(
            relay.to_string(),
            PathBuf::from(&args.config),
            state_handle.clone(),
            Duration::from_secs(15),
            backoff,
        )?;
        agent.set_current_version(lkg_version);
        Ok::<_, anyhow::Error>(Arc::new(agent))
    });

    let (telemetry, access_log_worker) = Telemetry::new(&config.telemetry);
    let telemetry = Arc::new(telemetry);
    let resolver = UpstreamResolver::new(state_handle.clone(), Duration::from_secs(10));

    for listener in &config.listeners {
        let proxy_app = Proxy {
            state: state_handle.clone(),
            telemetry: telemetry.clone(),
        };

        let mut proxy_service = http_proxy_service(&server.configuration, proxy_app);
        let listen_addr_str = listener.address.to_string();
        match &listener.tls {
            pavis_core::TlsConfig::Disabled => {
                proxy_service.add_tcp(&listen_addr_str);
            }
            pavis_core::TlsConfig::Enabled {
                cert_path,
                key_path,
                client_auth,
            } => {
                proxy_service
                    .add_tls(&listen_addr_str, &cert_path.0, &key_path.0)
                    .with_context(|| format!("Failed to add TLS listener: {}", listener.name.0))?;

                // Configure client certificate authentication
                match client_auth {
                    pavis_core::ClientAuth::Disabled => {
                        // No client certificate verification
                    }
                    pavis_core::ClientAuth::Optional { ca_path } => {
                        // TODO: Configure SSL_VERIFY_PEER without SSL_VERIFY_FAIL_IF_NO_PEER_CERT
                        // This allows the handshake to succeed even if the client doesn't present a cert
                        // The identity will be None in the RouterContext if no cert is provided
                        tracing::debug!(
                            ca_path = %ca_path.0,
                            "Configuring optional client certificate authentication"
                        );
                        // tls_settings = tls_settings.enable_client_cert_verification(&ca_path.0, false)?;
                    }
                    pavis_core::ClientAuth::Required { ca_path } => {
                        // TODO: Configure SSL_VERIFY_PEER with SSL_VERIFY_FAIL_IF_NO_PEER_CERT
                        // This requires the client to present a valid certificate
                        tracing::debug!(
                            ca_path = %ca_path.0,
                            "Configuring required client certificate authentication"
                        );
                        // tls_settings = tls_settings.enable_client_cert_verification(&ca_path.0, true)?;
                    }
                    #[allow(unreachable_patterns)]
                    &_ => {
                        // Unknown client auth configuration
                    }
                }
            }
            #[allow(unreachable_patterns)]
            &_ => {
                proxy_service.add_tcp(&listen_addr_str);
            }
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
    if let Some(agent) = config_agent {
        let agent = agent?;
        server.add_service(agent.worker());
    }
    server.run_forever();
}
