use clap::Parser;
use pavis_testkit::common::{cli::RelayArgs, logging};
use pavis_testkit::relay::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    let args = RelayArgs::parse();
    server::run(args).await
}
