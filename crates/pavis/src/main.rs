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
use pavis_core::{AccessLogConfig, LogLevel};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
    #[arg(long)]
    relay_url: Option<String>,
}

fn log_level_to_str(level: Option<LogLevel>) -> &'static str {
    match level {
        Some(LogLevel::Error) => "error",
        Some(LogLevel::Warn) => "warn",
        Some(LogLevel::Info) => "info",
        Some(LogLevel::Debug) => "debug",
        Some(LogLevel::Trace) => "trace",
        None => "info",
    }
}

#[cfg(test)]
mod tests {
    use super::log_level_to_str;
    use pavis_core::{AccessLogConfig, LogLevel};

    #[test]
    fn log_level_to_str_defaults_to_info() {
        assert_eq!(log_level_to_str(None), "info");
    }

    #[test]
    fn log_level_to_str_maps_values() {
        assert_eq!(log_level_to_str(Some(LogLevel::Error)), "error");
        assert_eq!(log_level_to_str(Some(LogLevel::Warn)), "warn");
        assert_eq!(log_level_to_str(Some(LogLevel::Info)), "info");
        assert_eq!(log_level_to_str(Some(LogLevel::Debug)), "debug");
        assert_eq!(log_level_to_str(Some(LogLevel::Trace)), "trace");
    }

    #[test]
    fn access_log_description_logic() {
        let desc = match AccessLogConfig::Disabled {
            AccessLogConfig::Disabled => "off".to_string(),
            AccessLogConfig::Stdout => "stdout".to_string(),
            AccessLogConfig::File(path) => format!("file:{}", path),
        };
        assert_eq!(desc, "off");

        let desc = match AccessLogConfig::Stdout {
            AccessLogConfig::Disabled => "off".to_string(),
            AccessLogConfig::Stdout => "stdout".to_string(),
            AccessLogConfig::File(path) => format!("file:{}", path),
        };
        assert_eq!(desc, "stdout");

        let desc = match AccessLogConfig::File("/tmp/test".to_string()) {
            AccessLogConfig::Disabled => "off".to_string(),
            AccessLogConfig::Stdout => "stdout".to_string(),
            AccessLogConfig::File(path) => format!("file:{}", path),
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

    if let Some(p_level) = config.telemetry.pingora {
        let p_str = log_level_to_str(Some(p_level));
        filter = filter
            .add_directive(format!("pingora={}", p_str).parse()?)
            .add_directive(format!("pingora_core={}", p_str).parse()?);
    }

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let access_log_desc = match &config.telemetry.access_log {
        AccessLogConfig::Disabled => "off".to_string(),
        AccessLogConfig::Stdout => "stdout".to_string(),
        AccessLogConfig::File(path) => format!("file:{}", path),
    };

    // Listener selection logic
    if config.listeners.is_empty() {
        anyhow::bail!("No listeners configured in runtime config.");
    }

    let max_threads = config
        .listeners
        .iter()
        .filter_map(|l| l.worker_threads)
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
        let listen_addr_str = listener.listen_addr.to_string();
        if let Some(tls_config) = &listener.tls {
            if tls_config.enabled {
                // Core validation guarantees cert/key presence when enabled.
                let cert_path = tls_config
                    .cert_path
                    .as_ref()
                    .expect("core validation must ensure cert_path when TLS enabled");
                let key_path = tls_config
                    .key_path
                    .as_ref()
                    .expect("core validation must ensure key_path when TLS enabled");
                proxy_service
                    .add_tls(&listen_addr_str, cert_path, key_path)
                    .with_context(|| format!("Failed to add TLS listener: {}", listener.name))?;
            } else {
                proxy_service.add_tcp(&listen_addr_str);
            }
        } else {
            proxy_service.add_tcp(&listen_addr_str);
        }

        tracing::info!(
            name = %listener.name,
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
