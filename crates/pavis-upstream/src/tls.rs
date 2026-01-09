use std::path::Path;

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use tokio::fs;

use crate::config::TlsConfigPaths;

pub async fn rustls_config(paths: &TlsConfigPaths) -> Result<RustlsConfig> {
    let cert = read_file(paths.cert_path()).await?;
    let key = read_file(paths.key_path()).await?;
    RustlsConfig::from_pem(cert, key)
        .await
        .context("failed to build rustls config from provided files")
}

async fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs::read(path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))
}
