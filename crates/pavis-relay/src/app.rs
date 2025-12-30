use crate::config;
use crate::routes::serve;
use crate::state::{RelayOptions, RelayState};
use anyhow::{Context, Result};
use axum::body::Bytes;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub async fn serve_from_config(config: &config::RelayConfig) -> Result<()> {
    let (listen_addr, state) = init_state(config)?;
    serve(listen_addr, state)
        .await
        .context("relay server failed")
}

fn init_state(config: &config::RelayConfig) -> Result<(SocketAddr, RelayState)> {
    let listen_addr: SocketAddr = config.http.bind.parse().context("invalid listen address")?;

    let lkg_path = resolve_lkg_path(config);
    let bytes = match std::fs::read(&lkg_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read LKG: {}", lkg_path.display()));
        }
    };

    let mut options = build_options(config).context("invalid relay config options")?;
    options.lkg_path = Some(lkg_path);
    let state = RelayState::new_with_options(0, Bytes::from(bytes), options)
        .context("failed to initialize relay state")?;
    Ok((listen_addr, state))
}

fn resolve_lkg_path(config: &config::RelayConfig) -> PathBuf {
    let lkg_path = PathBuf::from(&config.artifact.lkg_path);
    if lkg_path.is_absolute() || config.storage.root_dir.is_empty() {
        return lkg_path;
    }
    Path::new(&config.storage.root_dir).join(lkg_path)
}

fn build_options(config: &config::RelayConfig) -> Result<RelayOptions> {
    Ok(RelayOptions {
        version_header: axum::http::HeaderName::from_static(pavis_pvs::PAVIS_VERSION_HEADER),
        checksum_header: axum::http::HeaderName::from_static(pavis_pvs::PAVIS_CHECKSUM_HEADER),
        checksum_alg_header: axum::http::HeaderName::from_static(
            pavis_pvs::PAVIS_CHECKSUM_ALG_HEADER,
        ),
        long_poll_enabled: config.distribution.long_poll.enabled,
        identity_name: config.identity.name.clone(),
        lkg_path: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{build_options, init_state, resolve_lkg_path};
    use crate::config::RelayConfig;

    fn minimal_config() -> RelayConfig {
        RelayConfig {
            artifact: crate::config::ArtifactConfig {
                lkg_path: "config.pvs".to_string(),
                ..Default::default()
            },
            distribution: crate::config::DistributionConfig {
                long_poll: crate::config::LongPollConfig {
                    enabled: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            identity: crate::config::IdentityConfig {
                name: "relay".to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn resolve_lkg_path_respects_storage_root() {
        let mut config = minimal_config();
        config.storage.root_dir = "/var/lib/pavis".to_string();
        let path = resolve_lkg_path(&config);
        assert!(path.ends_with("config.pvs"));
        assert!(path.to_string_lossy().contains("/var/lib/pavis"));
    }

    #[test]
    fn build_options_uses_config_headers() {
        let config = minimal_config();
        let options = build_options(&config).expect("options");
        assert_eq!(
            options.version_header.as_str(),
            pavis_pvs::PAVIS_VERSION_HEADER
        );
        assert_eq!(
            options.checksum_header.as_str(),
            pavis_pvs::PAVIS_CHECKSUM_HEADER
        );
        assert_eq!(
            options.checksum_alg_header.as_str(),
            pavis_pvs::PAVIS_CHECKSUM_ALG_HEADER
        );
        assert!(options.long_poll_enabled);
        assert_eq!(options.identity_name, "relay");
    }

    #[test]
    fn init_state_reads_missing_lkg_as_empty() {
        let mut config = minimal_config();
        config.http.bind = "127.0.0.1:0".to_string();
        let (addr, state) = init_state(&config).expect("state");
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_eq!(state.options().identity_name, "relay");
    }

    #[test]
    fn init_state_rejects_invalid_listen_addr() {
        let mut config = minimal_config();
        config.http.bind = "bad-addr".to_string();
        let err = init_state(&config).err().expect("invalid listen addr");
        assert!(err.to_string().contains("invalid listen address"));
    }
}
