mod config;

use anyhow::{Context, Result};
use axum::body::Bytes;
use clap::Parser;
use pavis_relay::{RelayState, serve};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

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
    let listen_addr: SocketAddr = config.http.bind.parse().context("invalid listen address")?;

    let lkg_path = resolve_lkg_path(&config);
    let bytes = match std::fs::read(&lkg_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read LKG: {}", lkg_path.display()));
        }
    };

    let options = build_options(&config).context("invalid relay config options")?;
    let state = RelayState::new_with_options(0, Bytes::from(bytes), options)
        .context("failed to initialize relay state")?;

    serve(listen_addr, state)
        .await
        .context("relay server failed")
}

fn resolve_lkg_path(config: &config::RelayConfig) -> PathBuf {
    let lkg_path = PathBuf::from(&config.artifact.lkg_path);
    if lkg_path.is_absolute() || config.storage.root_dir.is_empty() {
        return lkg_path;
    }
    Path::new(&config.storage.root_dir).join(lkg_path)
}

fn build_options(config: &config::RelayConfig) -> Result<pavis_relay::RelayOptions> {
    let version_name = header_name_or_default(
        &config.distribution.long_poll.headers.version,
        "x-pavis-version",
    )?;
    let checksum_name = header_name_or_default(
        &config.distribution.long_poll.headers.checksum,
        "x-pavis-checksum",
    )?;
    let alg_name = header_name_or_default(
        config
            .distribution
            .long_poll
            .headers
            .algorithm
            .as_deref()
            .unwrap_or("x-pavis-checksum-alg"),
        "x-pavis-checksum-alg",
    )?;

    Ok(pavis_relay::RelayOptions {
        version_header: version_name,
        checksum_header: checksum_name,
        checksum_alg_header: alg_name,
        long_poll_enabled: config.distribution.long_poll.enabled,
        identity_name: config.identity.name.clone(),
    })
}

fn header_name_or_default(raw: &str, fallback: &str) -> Result<axum::http::HeaderName> {
    let value = if raw.trim().is_empty() { fallback } else { raw };
    axum::http::HeaderName::from_bytes(value.as_bytes()).context("invalid header name")
}
