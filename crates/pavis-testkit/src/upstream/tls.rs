use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use axum_server::tls_openssl::OpenSSLConfig;

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

pub fn openssl_config(paths: &TlsConfigPaths) -> Result<OpenSSLConfig> {
    OpenSSLConfig::from_pem_file(paths.cert_path(), paths.key_path())
        .context("failed to build OpenSSL config from provided files")
}
