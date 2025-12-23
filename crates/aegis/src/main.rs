use anyhow::{Context, Result};
use clap::Parser;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use std::sync::Arc;

mod config;
mod proxy;

use config::AegisConfig;
use proxy::MyProxy;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    let config_content =
        std::fs::read_to_string(&args.config).context("Failed to read config file")?;
    let config: AegisConfig =
        serde_yaml::from_str(&config_content).context("Failed to parse config file")?;

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

    tracing::info!(
        "Aegis starts on {} using {}",
        config.server.listen_addr,
        args.config
    );

    let mut my_server = Server::new(None).context("Failed to create Pingora server")?;
    my_server.bootstrap();

    let mut my_proxy = http_proxy_service(
        &my_server.configuration,
        MyProxy {
            config: config.clone(),
        },
    );
    my_proxy.add_tcp(&config.server.listen_addr);

    my_server.add_service(my_proxy);
    my_server.run_forever();
}
