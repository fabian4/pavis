use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Parser;
use pingora::prelude::*;
use pingora::proxy::{http_proxy_service, ProxyHttp, Session};
use std::sync::Arc;

mod config;
use config::AegisConfig;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
}

pub struct MyProxy {
    pub config: Arc<AegisConfig>,
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
        tracing::info!("Selecting upstream peer");
        // MVP: Just pick the first endpoint of the first upstream
        // In the future, we will implement full routing (Host -> Path -> Split)
        if let Some(upstream) = self.config.upstreams.first() {
            if let Some(endpoint) = upstream.endpoints.first() {
                let addr = format!("{}:{}", endpoint.ip, endpoint.port);
                let peer = Box::new(HttpPeer::new(
                    &addr,
                    false, // TLS disabled for now
                    "localhost".to_string(),
                ));
                return Ok(peer);
            }
        }

        // Fallback or error if no upstream found
        // For MVP, we'll error out if config is empty, but safely.
        pingora::Error::e_explain(pingora::ErrorType::InternalError, "No upstream configured")
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<()> {
        // tracing::info!("Filtering upstream request: method={}, uri={}", upstream_request.method, upstream_request.uri);
        // Basic header forwarding / modification
        upstream_request.insert_header("X-Proxy-By", "Aegis")?;
        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config_content = std::fs::read_to_string(&args.config)
        .with_context(|| format!("Failed to read config file: {}", args.config))?;
    let config: AegisConfig = serde_yaml::from_str(&config_content)
        .with_context(|| format!("Failed to parse config file: {}", args.config))?;
    let config = Arc::new(config);

    let mut filter = tracing_subscriber::EnvFilter::from_default_env();
    if !config.telemetry.pingora_log {
        // Disable pingora logs if pingora_log is false
        filter = filter
            .add_directive("pingora=off".parse().unwrap())
            .add_directive("pingora_core=off".parse().unwrap());
    }

    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!(
        "Aegis starts on {} using {}",
        config.server.listen_addr,
        args.config
    );

    let mut my_server = Server::new(None)?;
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
