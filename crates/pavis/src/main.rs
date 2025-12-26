use anyhow::{Context, Result};
use clap::Parser;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use std::sync::Arc;

mod config;
mod proxy;

use config::{AccessLogConfig, Config};
use proxy::Proxy;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    // TODO: Support config file watching for hot reload
    let config_content =
        std::fs::read_to_string(&args.config).context("Failed to read config file")?;
    let mut config: Config =
        serde_yaml::from_str(&config_content).context("Failed to parse config file")?;

    // Compile regex routes
    config.compile_routes()?;

    // TODO: Validate config (e.g., upstreams referenced in routes exist)
    let config = Arc::new(config);

    let log_level = config.telemetry.level.as_deref().unwrap_or("info");
    let mut filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    if let Some(p_level) = &config.telemetry.pingora {
        filter = filter
            .add_directive(format!("pingora={}", p_level).parse()?)
            .add_directive(format!("pingora_core={}", p_level).parse()?);
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

    let mut server = Server::new(None).context("Failed to create Pingora server")?;
    if let Some(threads) = config.server.worker_threads {
        if let Some(conf) = Arc::get_mut(&mut server.configuration) {
            conf.threads = threads;
        }
    }
    server.bootstrap();

    let mut upstream_counters = std::collections::HashMap::new();
    let mut upstreams = std::collections::HashMap::new();
    for upstream in &config.upstreams {
        upstream_counters.insert(
            upstream.name.clone(),
            std::sync::atomic::AtomicUsize::new(0),
        );
        upstreams.insert(upstream.name.clone(), upstream.clone());
    }

    let access_log_file = if let AccessLogConfig::File(path) = &config.telemetry.access_log {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("Failed to open access log file")?;
        Some(Arc::new(tokio::sync::Mutex::new(tokio::io::BufWriter::new(
            tokio::fs::File::from_std(file),
        ))))
    } else {
        None
    };

    let mut proxy_service = http_proxy_service(
        &server.configuration,
        Proxy {
            config: config.clone(),
            upstreams,
            upstream_counters,
            access_log_file,
        },
    );
    // TODO: Support multiple listen addresses
    if let Some(tls_config) = &config.server.tls {
        if tls_config.enabled {
            let cert_path = tls_config
                .cert_path
                .as_ref()
                .context("TLS enabled but cert_path is missing")?;
            let key_path = tls_config
                .key_path
                .as_ref()
                .context("TLS enabled but key_path is missing")?;
            proxy_service
                .add_tls(&config.server.listen_addr, cert_path, key_path)
                .context("Failed to add TLS listener")?;
        } else {
            proxy_service.add_tcp(&config.server.listen_addr);
        }
    } else {
        proxy_service.add_tcp(&config.server.listen_addr);
    }

    server.add_service(proxy_service);
    server.run_forever();
}
