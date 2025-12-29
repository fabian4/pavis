use anyhow::{Context, Result};
use std::net::SocketAddr;

use pavis_core::ServerConfig as RuntimeServerConfig;

use crate::config::types::{ServerConfig, TlsConfig};

pub(super) fn to_runtime(server: ServerConfig) -> Result<RuntimeServerConfig> {
    let listen_addr: SocketAddr = server.listen_addr.parse().context("Invalid listen_addr")?;

    Ok(RuntimeServerConfig {
        listen_addr,
        worker_threads: server.worker_threads.map(|w| w as u64),
        tls: server.tls.map(|t| pavis_core::TlsConfig {
            enabled: t.enabled,
            cert_path: t.cert_path,
            key_path: t.key_path,
        }),
    })
}

pub(super) fn from_runtime(server: RuntimeServerConfig) -> ServerConfig {
    ServerConfig {
        listen_addr: server.listen_addr.to_string(),
        worker_threads: server.worker_threads.map(|w| w as usize),
        tls: server.tls.map(|t| TlsConfig {
            enabled: t.enabled,
            cert_path: t.cert_path,
            key_path: t.key_path,
        }),
    }
}
