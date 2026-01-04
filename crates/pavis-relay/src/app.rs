use crate::config;
use crate::config::PersistenceOptions;
use crate::routes::serve;
use crate::state::{RelayOptions, RelayState};
use anyhow::{Context, Result};
use axum::body::Bytes;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub async fn serve_from_config(config: &config::RelayConfig) -> Result<()> {
    let (listen_addr, state) = init_state(config)?;
    crate::pipeline::start_pipeline(&config.pipeline, state.clone()).await?;
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
    let initial_version = if bytes.is_empty() { 0 } else { 1 };
    let state = RelayState::new_with_options(initial_version, Bytes::from(bytes), options)
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
    if config.persistence.flush_interval == 0 {
        anyhow::bail!("persistence.flush_interval must be greater than zero");
    }
    if config.persistence.retry.backoff.min == 0 {
        anyhow::bail!("persistence.retry.backoff.min must be greater than zero");
    }
    if config.persistence.retry.backoff.max < config.persistence.retry.backoff.min {
        anyhow::bail!("persistence.retry.backoff.max must be >= persistence.retry.backoff.min");
    }

    Ok(RelayOptions {
        version_header: axum::http::HeaderName::from_static(pavis_pvs::PAVIS_VERSION_HEADER),
        checksum_header: axum::http::HeaderName::from_static(pavis_pvs::PAVIS_CHECKSUM_HEADER),
        checksum_alg_header: axum::http::HeaderName::from_static(
            pavis_pvs::PAVIS_CHECKSUM_ALG_HEADER,
        ),
        long_poll_enabled: config.distribution.long_poll.enabled,
        identity_name: config.identity.name.clone(),
        lkg_path: None,
        persistence: PersistenceOptions {
            enabled: config.persistence.enabled,
            flush_interval: Duration::from_millis(config.persistence.flush_interval),
            retry_max: config.persistence.retry.max,
            retry_backoff: Duration::from_millis(config.persistence.retry.backoff.min),
            retry_backoff_max: Duration::from_millis(config.persistence.retry.backoff.max),
        },
        max_pvs_bytes: config.artifact.limits.max_pvs_bytes,
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

    fn init_state_reads_existing_lkg() {
        let dir = std::env::temp_dir().join("relay_lkg_test");

        let _ = std::fs::remove_dir_all(&dir);

        std::fs::create_dir_all(&dir).unwrap();

        let lkg = dir.join("config.pvs");

        let runtime_config = pavis_core::RuntimeConfig {
            listeners: vec![pavis_core::Listener {
                name: "default".to_string(),

                listen_addr: "127.0.0.1:8080".parse().unwrap(),

                worker_threads: None,

                tls: None,
            }],

            telemetry: pavis_core::TelemetryConfig {
                level: None,

                pingora: None,

                service_name: None,

                prometheus_addr: None,

                access_log: pavis_core::AccessLogConfig::Disabled,

                tracing: None,
            },

            upstreams: vec![],

            routes: vec![],
        };

        pavis_pvs::write(&lkg, &runtime_config).unwrap();

        let mut config = minimal_config();

        config.http.bind = "127.0.0.1:0".to_string();

        config.artifact.lkg_path = lkg.to_string_lossy().to_string();

        let (_addr, state) = init_state(&config).expect("state");

        let snapshot = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(state.snapshot());

        assert!(!snapshot.pvs_bytes.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_options_rejects_invalid_persistence_config() {
        let mut config = minimal_config();

        // Zero flush interval
        config.persistence.flush_interval = 0;
        let err = build_options(&config).expect_err("zero flush");
        assert!(
            err.to_string()
                .contains("flush_interval must be greater than zero")
        );
        config.persistence.flush_interval = 1000; // Reset

        // Zero retry min backoff
        config.persistence.retry.backoff.min = 0;
        let err = build_options(&config).expect_err("zero min backoff");
        assert!(
            err.to_string()
                .contains("backoff.min must be greater than zero")
        );
        config.persistence.retry.backoff.min = 100; // Reset

        // Max < Min backoff
        config.persistence.retry.backoff.min = 200;
        config.persistence.retry.backoff.max = 100;
        let err = build_options(&config).expect_err("max < min");
        assert!(err.to_string().contains("max must be >="));
    }

    #[test]
    fn init_state_fails_on_lkg_read_error() {
        let dir = std::env::temp_dir().join("relay_lkg_fail");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Use a directory as the LKG path to force a read error (EISDIR on Unix, Access Denied on Windows)
        let lkg = dir.join("config.pvs");
        std::fs::create_dir(&lkg).unwrap();

        let mut config = minimal_config();
        config.http.bind = "127.0.0.1:0".to_string();
        config.artifact.lkg_path = lkg.to_string_lossy().to_string();

        let err = init_state(&config).err().expect("lkg error");

        // Debug the actual error content
        dbg!(&err); // Print the actual error

        assert!(err.to_string().contains("failed to read LKG"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_serve_from_config_abort() {
        let mut config = minimal_config();
        config.http.bind = "127.0.0.1:0".to_string(); // Random port

        let handle = tokio::spawn(async move { super::serve_from_config(&config).await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        handle.abort();
    }
}
