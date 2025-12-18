use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use pingora::prelude::*;
use pingora::proxy::{http_proxy_service, ProxyHttp, Session};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
}

pub struct MyProxy {
    // We'll expand this as we implement the internal protocol
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
        // Hardcoded for now to prove it runs
        let peer = Box::new(HttpPeer::new(
            "127.0.0.1:3000",
            false,
            "localhost".to_string(),
        ));
        Ok(peer)
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    println!("Starting proxy with config: {}", args.config);

    let mut my_server = Server::new(None)?;
    my_server.bootstrap();

    let mut my_proxy = http_proxy_service(&my_server.configuration, MyProxy {});
    my_proxy.add_tcp("0.0.0.0:8080");

    my_server.add_service(my_proxy);
    my_server.run_forever();
}
