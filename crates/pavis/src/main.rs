use anyhow::{Context, Result};
use clap::Parser;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use pingora::server::configuration::ServerConf;
use std::sync::Arc;

use pavis::load::{self, LoadError};
use pavis::proxy::Proxy;
use pavis::router::Router;
use pavis::telemetry::Telemetry;
use pavis::upstream::Manager;
use pavis_core::{AccessLogConfig, LogLevel};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
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
    use pavis_core::LogLevel;

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
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    // TODO: Support config file watching for hot reload
    let config = load::load_file(&args.config).map_err(|e| match e {
        LoadError::VersionMismatch { file, expected } => anyhow::anyhow!(
            "Version mismatch! File: {}, Runtime expects: {}. Recompile config.",
            file,
            expected
        ),
        other => anyhow::anyhow!(other),
    })?;

    // Initialize Router (compiles regexes)
    let router = Arc::new(Router::new(config.routes.clone())?);

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
        AccessLogConfig::False => "off".to_string(),
        AccessLogConfig::Stdout => "stdout".to_string(),
        AccessLogConfig::File(path) => format!("file:{}", path),
    };

    tracing::info!(
        listen = %config.server.listen_addr,
        config = %args.config,
        access_log = %access_log_desc,
        "Pavis starting"
    );

    let mut server_conf = ServerConf {
        daemon: false,
        ..Default::default()
    };
    if let Some(threads) = config.server.worker_threads {
        server_conf.threads = threads as usize;
    }
    let mut server = Server::new_with_opt_and_conf(None, server_conf);
    server.bootstrap();

    let upstream_manager = Manager::new(&config.upstreams);

    let (telemetry, access_log_worker) = Telemetry::new(&config.telemetry);
    let telemetry = Arc::new(telemetry);

    let mut proxy_service = http_proxy_service(
        &server.configuration,
        Proxy {
            router,
            upstream_manager,
            telemetry,
        },
    );
    // TODO: Support multiple listen addresses
    let listen_addr_str = config.server.listen_addr.to_string();
    if let Some(tls_config) = &config.server.tls {
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
                .context("Failed to add TLS listener")?;
        } else {
            proxy_service.add_tcp(&listen_addr_str);
        }
    } else {
        proxy_service.add_tcp(&listen_addr_str);
    }

    server.add_service(access_log_worker);
    server.add_service(proxy_service);
    server.run_forever();
}
