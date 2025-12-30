use anyhow::{Context, Result};
use clap::Parser;
use pavis_relay::{config, serve_from_config};
use std::path::Path;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "relay.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = config::load(Path::new(&args.config)).context("failed to load relay config")?;
    serve_from_config(&config).await
}
