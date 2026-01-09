use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use tokio::fs;

#[derive(Debug, Clone)]
pub struct TlsConfigPaths {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl TlsConfigPaths {
    pub fn cert_path(&self) -> &Path {
        &self.cert_path
    }

    pub fn key_path(&self) -> &Path {
        &self.key_path
    }
}

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
