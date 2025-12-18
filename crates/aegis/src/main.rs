use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Parser;
use pingora::prelude::*;
use pingora::proxy::{http_proxy_service, ProxyHttp, Session};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub listen: String,
    pub upstream: String,
}

pub struct MyProxy {
    pub config: Arc<Config>,
}

#[async_trait]
impl ProxyHttp for MyProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<Box<HttpPeer>> {
        let peer = Box::new(HttpPeer::new(&self.config.upstream, false, String::new()));
        Ok(peer)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<()> {
        // Basic header forwarding / modification
        upstream_request.insert_header("X-Proxy-By", "Aegis")?;
        Ok(())
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let config_content = std::fs::read_to_string(&args.config)
        .with_context(|| format!("Failed to read config file: {}", args.config))?;
    let config: Config = serde_yaml::from_str(&config_content)
        .with_context(|| format!("Failed to parse config file: {}", args.config))?;
    let config = Arc::new(config);

    tracing::info!("Starting proxy with config: {:?}", config);

    let mut my_server = Server::new(None)?;
    my_server.bootstrap();

    let mut my_proxy = http_proxy_service(
        &my_server.configuration,
        MyProxy {
            config: config.clone(),
        },
    );
    my_proxy.add_tcp(&config.listen);

    my_server.add_service(my_proxy);
    my_server.run_forever();
}
