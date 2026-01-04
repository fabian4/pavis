use anyhow::{Context, Result};
use std::net::SocketAddr;

use pavis_core::Listener as RuntimeListener;

use crate::config::types::{Listener, TlsConfig};

pub(super) fn to_runtime(listener: Listener) -> Result<RuntimeListener> {
    let listen_addr: SocketAddr = listener
        .listen_addr
        .parse()
        .context("Invalid listen_addr")?;

    Ok(RuntimeListener {
        name: listener.name,
        listen_addr,
        worker_threads: listener.worker_threads.map(|w| w as u64),
        tls: listener.tls.map(|t| pavis_core::TlsConfig {
            enabled: t.enabled,
            cert_path: t.cert_path,
            key_path: t.key_path,
        }),
    })
}

pub(super) fn from_runtime(listener: RuntimeListener) -> Listener {
    Listener {
        name: listener.name,
        listen_addr: listener.listen_addr.to_string(),
        worker_threads: listener.worker_threads.map(|w| w as usize),
        tls: listener.tls.map(|t| TlsConfig {
            enabled: t.enabled,
            cert_path: t.cert_path,
            key_path: t.key_path,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{from_runtime, to_runtime};
    use crate::config::types::{Listener, TlsConfig};

    #[test]
    fn to_runtime_maps_tls_fields() {
        let listener = Listener {
            name: "default".to_string(),
            listen_addr: "127.0.0.1:8080".to_string(),
            worker_threads: Some(2),
            tls: Some(TlsConfig {
                enabled: true,
                cert_path: Some("/tmp/cert.pem".to_string()),
                key_path: Some("/tmp/key.pem".to_string()),
            }),
        };
        let runtime = to_runtime(listener).expect("runtime");
        let tls = runtime.tls.expect("tls");
        assert!(tls.enabled);
        assert_eq!(tls.cert_path.as_deref(), Some("/tmp/cert.pem"));
        assert_eq!(tls.key_path.as_deref(), Some("/tmp/key.pem"));
    }

    #[test]
    fn from_runtime_maps_tls_fields() {
        let runtime = pavis_core::Listener {
            name: "default".to_string(),
            listen_addr: "127.0.0.1:8080".parse().expect("addr"),
            worker_threads: Some(1),
            tls: Some(pavis_core::TlsConfig {
                enabled: false,
                cert_path: Some("/tmp/cert.pem".to_string()),
                key_path: Some("/tmp/key.pem".to_string()),
            }),
        };
        let listener = from_runtime(runtime);
        let tls = listener.tls.expect("tls");
        assert!(!tls.enabled);
        assert_eq!(tls.cert_path.as_deref(), Some("/tmp/cert.pem"));
        assert_eq!(tls.key_path.as_deref(), Some("/tmp/key.pem"));
    }
}
