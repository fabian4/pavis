use clap::Parser;
use pavis_testkit::common::{cli::UpstreamArgs, logging};
use pavis_testkit::upstream::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    let args = UpstreamArgs::parse().resolve()?;
    server::run(args).await
}
