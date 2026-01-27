use clap::Parser;
use pavis_testkit::common::{cli::UpstreamArgs, logging};
use pavis_testkit::upstream::server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();
    let args = UpstreamArgs::parse().resolve()?;
    server::run(args).await
}
