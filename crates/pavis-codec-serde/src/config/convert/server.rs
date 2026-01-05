use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::num::NonZeroU16;

use pavis_core::{Listener as RuntimeListener, ListenerName, Path, TlsConfig, WorkerCount};

use crate::config::types::{Listener, TlsConfig as SerdeTls};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{Listener, TlsConfig as SerdeTls};
    use pavis_core::{ListenerName, Path, TlsConfig, WorkerCount};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::NonZeroU16;

    #[test]
    fn to_runtime_validates_address() {
        let listener = Listener {
            name: "default".to_string(),
            address: "invalid-ip".to_string(),
            workers: None,
            tls: None,
        };
        let err = to_runtime(listener).unwrap_err();
        assert!(err.to_string().contains("Invalid address"));
    }

    #[test]
    fn to_runtime_validates_workers() {
        let listener = Listener {
            name: "default".to_string(),
            address: "127.0.0.1:8080".to_string(),
            workers: Some(0),
            tls: None,
        };
        let err = to_runtime(listener).unwrap_err();
        assert!(err.to_string().contains("workers must be > 0"));
    }

    #[test]
    fn to_runtime_validates_tls_fields() {
        let listener = Listener {
            name: "default".to_string(),
            address: "127.0.0.1:8080".to_string(),
            workers: None,
            tls: Some(SerdeTls {
                cert_path: None,
                key_path: None,
            }),
        };
        let err = to_runtime(listener).unwrap_err();
        assert!(err.to_string().contains("tls.cert_path is required"));
    }

    #[test]
    fn from_runtime_round_trips_workers_and_tls() {
        let runtime = pavis_core::Listener {
            name: ListenerName("default".to_string()),
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
            workers: WorkerCount::Count(NonZeroU16::new(4).unwrap()),
            tls: TlsConfig::Enabled {
                cert_path: Path("cert.pem".to_string()),
                key_path: Path("key.pem".to_string()),
            },
        };

        let serde = from_runtime(runtime);
        assert_eq!(serde.workers, Some(4));
        let tls = serde.tls.unwrap();
        assert_eq!(tls.cert_path.as_deref(), Some("cert.pem"));
        assert_eq!(tls.key_path.as_deref(), Some("key.pem"));
    }
}

pub(super) fn to_runtime(listener: Listener) -> Result<RuntimeListener> {
    let address: SocketAddr = listener.address.parse().context("Invalid address")?;

    let workers = match listener.workers {
        Some(count) => {
            let count =
                NonZeroU16::new(count).ok_or_else(|| anyhow::anyhow!("workers must be > 0"))?;
            WorkerCount::Count(count)
        }
        None => WorkerCount::Auto,
    };

    let tls = match listener.tls {
        None => TlsConfig::Disabled,
        Some(tls) => {
            let cert = tls
                .cert_path
                .ok_or_else(|| anyhow::anyhow!("tls.cert_path is required when tls is set"))?;
            let key = tls
                .key_path
                .ok_or_else(|| anyhow::anyhow!("tls.key_path is required when tls is set"))?;
            TlsConfig::Enabled {
                cert_path: Path(cert),
                key_path: Path(key),
            }
        }
    };

    Ok(RuntimeListener {
        name: ListenerName(listener.name),
        address,
        workers,
        tls,
    })
}

pub(super) fn from_runtime(listener: RuntimeListener) -> Listener {
    let workers = match listener.workers {
        WorkerCount::Auto => None,
        WorkerCount::Count(count) => Some(count.get()),
    };

    let tls = match listener.tls {
        TlsConfig::Disabled => None,
        TlsConfig::Enabled {
            cert_path,
            key_path,
        } => Some(SerdeTls {
            cert_path: Some(cert_path.0),
            key_path: Some(key_path.0),
        }),
    };

    Listener {
        name: listener.name.0,
        address: listener.address.to_string(),
        workers,
        tls,
    }
}
