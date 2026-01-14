use clap::Parser;
use pavis_testkit::common::{cli::RelayArgs, logging};
use pavis_testkit::relay::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    let args = RelayArgs::parse();
    server::run(args).await
}
