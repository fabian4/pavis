use anyhow::{Context, Result};
use clap::Parser;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use pingora::server::configuration::ServerConf;
use std::sync::Arc;

use pavis::config;
use pavis::proxy::Proxy;
use pavis::router::Router;
use pavis::telemetry::Telemetry;
use pavis::upstream::Manager;
use pavis_core::config::AccessLogConfig;

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
    let config = config::load_file(&args.config)?;

    // Initialize Router (compiles regexes)
    let router = Arc::new(Router::new(&config.routes)?);

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

    let mut server_conf = ServerConf {
        daemon: false,
        ..Default::default()
    };
    if let Some(threads) = config.server.worker_threads {
        server_conf.threads = threads;
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

    server.add_service(access_log_worker);
    server.add_service(proxy_service);
    server.run_forever();
}
