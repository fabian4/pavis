use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

const DEFAULT_HTTP_PORT: u16 = 8080;
const DEFAULT_HTTPS_PORT: u16 = 8443;
const DEFAULT_INSTANCE_ID: &str = "pavis-upstream";

#[derive(Debug, Clone)]
pub struct AppConfig {
    http_port: u16,
    https_port: u16,
    instance_id: String,
    tls_paths: TlsConfigPaths,
}

#[derive(Debug, Clone)]
pub struct TlsConfigPaths {
    cert_path: PathBuf,
    key_path: PathBuf,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let http_port = parse_port("HTTP_PORT", DEFAULT_HTTP_PORT)?;
        let https_port = parse_port("HTTPS_PORT", DEFAULT_HTTPS_PORT)?;
        let instance_id =
            env::var("INSTANCE_ID").unwrap_or_else(|_| DEFAULT_INSTANCE_ID.to_string());

        let cert_path = env::var("TLS_CERT_FILE")
            .context("TLS_CERT_FILE must be set and point to a PEM certificate")?
            .into();
        let key_path = env::var("TLS_KEY_FILE")
            .context("TLS_KEY_FILE must be set and point to a PEM private key")?
            .into();

        Ok(Self {
            http_port,
            https_port,
            instance_id,
            tls_paths: TlsConfigPaths {
                cert_path,
                key_path,
            },
        })
    }

    pub fn http_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), self.http_port)
    }

    pub fn https_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), self.https_port)
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn tls_paths(&self) -> &TlsConfigPaths {
        &self.tls_paths
    }
}

impl TlsConfigPaths {
    pub fn cert_path(&self) -> &Path {
        &self.cert_path
    }

    pub fn key_path(&self) -> &Path {
        &self.key_path
    }
}

fn parse_port(key: &str, default: u16) -> Result<u16> {
    match env::var(key) {
        Ok(value) => value
            .parse::<u16>()
            .with_context(|| format!("failed to parse {} as a port", key)),
        Err(_) => Ok(default),
    }
}
