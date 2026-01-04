use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::num::NonZeroU16;

use pavis_core::{Listener as RuntimeListener, ListenerName, Path, TlsConfig, WorkerCount};

use crate::config::types::{Listener, TlsConfig as SerdeTls};

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
