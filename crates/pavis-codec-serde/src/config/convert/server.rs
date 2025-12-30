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

#[cfg(test)]
mod tests {
    use super::{from_runtime, to_runtime};
    use crate::config::types::{ServerConfig, TlsConfig};

    #[test]
    fn to_runtime_maps_tls_fields() {
        let server = ServerConfig {
            listen_addr: "127.0.0.1:8080".to_string(),
            worker_threads: Some(2),
            tls: Some(TlsConfig {
                enabled: true,
                cert_path: Some("/tmp/cert.pem".to_string()),
                key_path: Some("/tmp/key.pem".to_string()),
            }),
        };
        let runtime = to_runtime(server).expect("runtime");
        let tls = runtime.tls.expect("tls");
        assert!(tls.enabled);
        assert_eq!(tls.cert_path.as_deref(), Some("/tmp/cert.pem"));
        assert_eq!(tls.key_path.as_deref(), Some("/tmp/key.pem"));
    }

    #[test]
    fn from_runtime_maps_tls_fields() {
        let runtime = pavis_core::ServerConfig {
            listen_addr: "127.0.0.1:8080".parse().expect("addr"),
            worker_threads: Some(1),
            tls: Some(pavis_core::TlsConfig {
                enabled: false,
                cert_path: Some("/tmp/cert.pem".to_string()),
                key_path: Some("/tmp/key.pem".to_string()),
            }),
        };
        let server = from_runtime(runtime);
        let tls = server.tls.expect("tls");
        assert!(!tls.enabled);
        assert_eq!(tls.cert_path.as_deref(), Some("/tmp/cert.pem"));
        assert_eq!(tls.key_path.as_deref(), Some("/tmp/key.pem"));
    }
}
