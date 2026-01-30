use crate::codec::BoxedCodec;
use crate::config::{self, PipelineConfig, PipelineOptions};
use crate::ingest::{BoxedIngest, boxed_ingest};
use crate::pipeline::start_pipeline;
use crate::routes::serve;
use crate::runtime::{RelayOptions, RelayRuntimeState};
use crate::state::{RelayState, derive_state_from_lkg, load_state, save_state};
use crate::storage::history::{find_corrupt_versions, find_orphaned_versions};
use crate::storage::lkg::{lkg_artifact_path, load_lkg, repair_lkg};
use crate::storage::validated_path::ValidatedStorageRoot;
use anyhow::{Context, Result};
use axum::body::Bytes;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::warn;

pub async fn serve_from_config(
    config: &config::RelayConfig,
    data_dir: Option<&Path>,
) -> Result<()> {
    let (listen_addr, state) = init_state(config, data_dir)?;
    let label = format!(
        "{:?}-{:?}",
        config.pipeline.ingest.source, config.pipeline.codec
    );
    let options = PipelineOptions::from_config(&config.pipeline);
    let ingest = build_ingest(&config.pipeline)?;
    let codec = build_codec(&config.pipeline)?;
    start_pipeline(label, ingest, codec, state.clone(), options).await?;
    serve(listen_addr, state)
        .await
        .context("relay server failed")
}

fn init_state(
    config: &config::RelayConfig,
    data_dir: Option<&Path>,
) -> Result<(SocketAddr, RelayRuntimeState)> {
    let listen_addr: SocketAddr = config.http.bind.parse().context("invalid listen address")?;

    let base_dir = resolve_data_dir(config, data_dir);
    ensure_storage_dirs(&base_dir)?;

    let storage_root = ValidatedStorageRoot::new(base_dir.clone())
        .context("failed to validate storage root path")?;

    repair_lkg(&storage_root).context("failed to repair LKG")?;

    let mut options = build_options(config).context("invalid relay config options")?;
    options.lkg_path = Some(lkg_artifact_path(&storage_root));
    options.storage_root = storage_root.clone();

    let (bytes, lkg_meta) = match load_lkg(&storage_root) {
        Ok(Some((bytes, meta))) => {
            if options.max_pvs_bytes > 0 && (bytes.len() as u64) > options.max_pvs_bytes {
                anyhow::bail!(
                    "LKG {} exceeds max_pvs_bytes {}",
                    options.lkg_path.as_ref().unwrap().display(),
                    options.max_pvs_bytes
                );
            }
            if meta.size != bytes.len() as u64 {
                warn!(
                    "LKG metadata size {} does not match artifact size {}",
                    meta.size,
                    bytes.len()
                );
            }
            (bytes, Some(meta))
        }
        Ok(None) => (Vec::new(), None),
        Err(err) => return Err(err).context("failed to load LKG"),
    };

    let derived_state = lkg_meta
        .as_ref()
        .map(derive_state_from_lkg)
        .unwrap_or(RelayState { current_version: 0 });
    let state_path = storage_root.as_path().join("state.json");
    let cached_state = load_state(&state_path).context("failed to load state.json")?;
    if cached_state.as_ref() != Some(&derived_state) {
        save_state(&state_path, &derived_state).context("failed to persist state.json")?;
    }

    let orphans = find_orphaned_versions(&storage_root, derived_state.current_version)
        .context("failed to scan history for orphans")?;
    for version in orphans {
        warn!("history entry version {} exceeds LKG version", version);
    }

    let corrupt =
        find_corrupt_versions(&storage_root).context("failed to scan history for corruption")?;
    for version in corrupt {
        warn!(
            "history entry version {} is missing .pvs or .meta.json",
            version
        );
    }

    let state = RelayRuntimeState::new_with_options(
        derived_state.current_version,
        Bytes::from(bytes),
        options,
    )
    .context("failed to initialize relay state")?;
    Ok((listen_addr, state))
}

fn build_codec(config: &PipelineConfig) -> Result<BoxedCodec> {
    match config.codec.kind {
        config::CodecKind::Serde => {
            #[cfg(feature = "codec-serde")]
            {
                Ok(Box::new(pavis_codec_serde::SerdeCodec {
                    format: pavis_codec_serde::SerdeFormat::Yaml,
                }))
            }
            #[cfg(not(feature = "codec-serde"))]
            {
                anyhow::bail!("codec-serde feature is disabled");
            }
        }
    }
}

fn build_ingest(config: &PipelineConfig) -> Result<BoxedIngest> {
    match &config.ingest.source {
        config::IngestSource::File(file_config) => {
            #[cfg(feature = "ingest-file")]
            {
                let debounce_ms = config.ingest.mode.debounce;
                let ingest = pavis_ingest_file::FileIngest::new(
                    &file_config.path,
                    Duration::from_millis(debounce_ms),
                );
                Ok(boxed_ingest(ingest))
            }
            #[cfg(not(feature = "ingest-file"))]
            {
                anyhow::bail!("ingest-file feature is disabled");
            }
        }
        config::IngestSource::None => {
            // For "none" source, create a dummy ingest that never produces artifacts
            // This is useful for testing scenarios where config is only published via HTTP API
            use futures_util::stream;
            use pavis_ingest_api::{Ingest, IngestError};

            struct NoneIngest;

            #[async_trait::async_trait]
            impl Ingest for NoneIngest {
                type Stream = std::pin::Pin<
                    Box<
                        dyn futures_util::Stream<
                                Item = Result<pavis_ingest_api::Artifact, IngestError>,
                            > + Send,
                    >,
                >;

                async fn stream(&mut self) -> Result<Self::Stream, IngestError> {
                    // Return a stream that never produces any items (just hangs forever)
                    Ok(Box::pin(stream::pending()))
                }
            }

            Ok(boxed_ingest(NoneIngest))
        }
    }
}

fn resolve_data_dir(config: &config::RelayConfig, data_dir: Option<&Path>) -> PathBuf {
    if let Some(data_dir) = data_dir
        && !data_dir.as_os_str().is_empty()
    {
        return data_dir.to_path_buf();
    }
    if !config.storage.root_dir.is_empty() {
        return PathBuf::from(&config.storage.root_dir);
    }
    PathBuf::from("/var/lib/pavis-relay")
}

fn ensure_storage_dirs(base_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(base_dir.join("lkg"))
        .with_context(|| format!("failed to create LKG dir under {}", base_dir.display()))?;
    std::fs::create_dir_all(base_dir.join("history"))
        .with_context(|| format!("failed to create history dir under {}", base_dir.display()))?;
    Ok(())
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

    // Note: storage_root will be set by init_state after validation
    // Use a unique temporary path that will be replaced by init_state
    let temp_storage = std::env::temp_dir().join(format!(
        "pavis-relay-temp-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let storage_root = ValidatedStorageRoot::new(temp_storage)
        .context("failed to create temporary storage root")?;

    Ok(RelayOptions {
        version_header: axum::http::HeaderName::from_static(pavis_core::CONFIG_VERSION_HEADER),
        checksum_header: axum::http::HeaderName::from_static(pavis_pvs::PAVIS_CHECKSUM_HEADER),
        checksum_alg_header: axum::http::HeaderName::from_static(
            pavis_pvs::PAVIS_CHECKSUM_ALG_HEADER,
        ),
        generated_at_header: axum::http::HeaderName::from_static(
            pavis_pvs::PAVIS_GENERATED_AT_HEADER,
        ),
        long_poll_enabled: config.distribution.long_poll.enabled,
        identity_name: config.identity.name.clone(),
        lkg_path: None,
        storage_root,
        max_pvs_bytes: config.artifact.limits.max_pvs_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::{build_options, init_state, resolve_data_dir};
    use crate::config::RelayConfig;
    use crate::state::load_state;
    use crate::storage::lkg::promote_to_lkg;
    use crate::storage::metadata::ArtifactMetadata;
    use crate::storage::validated_path::ValidatedStorageRoot;
    use std::time::{SystemTime, UNIX_EPOCH};

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

    fn sample_pvs_bytes() -> Vec<u8> {
        let listener = pavis_core::ListenerBuilder::new()
            .name(pavis_core::ListenerName("default".to_string()))
            .address("127.0.0.1:8080".parse().unwrap())
            .workers(pavis_core::WorkerCount::Auto)
            .tls(pavis_core::TlsConfig::Disabled)
            .build()
            .expect("listener");

        let runtime_config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(pavis_core::Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: pavis_core::ServiceName("pavis".to_string()),
                metrics: pavis_core::Metrics::Disabled,
                access_log: pavis_core::AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .expect("runtime config");

        let validated = pavis_core::validate_runtime(runtime_config).expect("validate");
        pavis_pvs::encode(validated.as_ref()).expect("encode")
    }

    #[test]
    fn resolve_data_dir_respects_storage_root() {
        let mut config = minimal_config();
        config.storage.root_dir = "/var/lib/pavis".to_string();
        let path = resolve_data_dir(&config, None);
        assert_eq!(path, std::path::PathBuf::from("/var/lib/pavis"));
    }

    #[test]
    fn build_options_uses_config_headers() {
        let config = minimal_config();
        let options = build_options(&config).expect("options");
        assert_eq!(
            options.version_header.as_str(),
            pavis_core::CONFIG_VERSION_HEADER
        );
        assert_eq!(
            options.checksum_header.as_str(),
            pavis_pvs::PAVIS_CHECKSUM_HEADER
        );
        assert_eq!(
            options.checksum_alg_header.as_str(),
            pavis_pvs::PAVIS_CHECKSUM_ALG_HEADER
        );
        assert_eq!(
            options.generated_at_header.as_str(),
            pavis_pvs::PAVIS_GENERATED_AT_HEADER
        );
        assert!(options.long_poll_enabled);
        assert_eq!(options.identity_name, "relay");
    }

    #[test]
    fn init_state_reads_missing_lkg_as_empty() {
        let mut config = minimal_config();
        config.http.bind = "127.0.0.1:0".to_string();
        let dir = std::env::temp_dir().join(format!(
            "relay_init_empty_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (addr, state) = init_state(&config, Some(&dir)).expect("state");
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_eq!(state.options().identity_name, "relay");
        let state_path = dir.join("state.json");
        let persisted = load_state(&state_path).expect("load state").expect("state");
        assert_eq!(persisted.current_version, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]

    fn init_state_reads_existing_lkg() {
        let dir = std::env::temp_dir().join(format!(
            "relay_lkg_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let _ = std::fs::remove_dir_all(&dir);

        std::fs::create_dir_all(&dir).unwrap();

        let pvs_bytes = sample_pvs_bytes();
        let meta = ArtifactMetadata {
            version: 1,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: crate::storage::metadata::checksum_for_bytes(&pvs_bytes),
            size: pvs_bytes.len() as u64,
        };
        let storage_root = ValidatedStorageRoot::new(dir.clone()).unwrap();
        promote_to_lkg(&storage_root, &pvs_bytes, &meta).unwrap();

        let mut config = minimal_config();

        config.http.bind = "127.0.0.1:0".to_string();

        let (_addr, state) = init_state(&config, Some(&dir)).expect("state");

        let snapshot = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(state.snapshot());

        assert!(!snapshot.pvs_bytes.is_empty());
        let state_path = dir.join("state.json");
        let persisted = load_state(&state_path).expect("load state").expect("state");
        assert_eq!(persisted.current_version, 1);

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
        let dir = std::env::temp_dir().join(format!(
            "relay_lkg_fail_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Create a corrupt LKG metadata (directory in place of file).
        let lkg_dir = dir.join("lkg");
        std::fs::create_dir_all(&lkg_dir).unwrap();
        let meta_dir = lkg_dir.join("meta.json");
        std::fs::create_dir(&meta_dir).unwrap();

        let mut config = minimal_config();
        config.http.bind = "127.0.0.1:0".to_string();

        let err = init_state(&config, Some(&dir)).err().expect("lkg error");

        assert!(err.to_string().contains("failed to repair LKG"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_state_rejects_oversized_lkg() {
        let dir = std::env::temp_dir().join(format!(
            "relay_lkg_oversize_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let pvs_bytes = vec![0u8; 32];
        let meta = ArtifactMetadata {
            version: 1,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: crate::storage::metadata::checksum_for_bytes(&pvs_bytes),
            size: pvs_bytes.len() as u64,
        };
        let storage_root = ValidatedStorageRoot::new(dir.clone()).unwrap();
        promote_to_lkg(&storage_root, &pvs_bytes, &meta).unwrap();

        let mut config = minimal_config();
        config.http.bind = "127.0.0.1:0".to_string();
        config.artifact.limits.max_pvs_bytes = 8;

        let err = init_state(&config, Some(&dir))
            .err()
            .expect("oversize error");
        assert!(err.to_string().contains("max_pvs_bytes"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn init_state_rewrites_state_json_on_mismatch() {
        let dir = std::env::temp_dir().join(format!(
            "runtime_mismatch_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let pvs_bytes = sample_pvs_bytes();
        let meta = ArtifactMetadata {
            version: 2,
            published_at: SystemTime::UNIX_EPOCH,
            checksum: crate::storage::metadata::checksum_for_bytes(&pvs_bytes),
            size: pvs_bytes.len() as u64,
        };
        let storage_root = ValidatedStorageRoot::new(dir.clone()).unwrap();
        promote_to_lkg(&storage_root, &pvs_bytes, &meta).unwrap();

        let state_path = dir.join("state.json");
        crate::state::save_state(
            &state_path,
            &crate::state::RelayState { current_version: 9 },
        )
        .unwrap();

        let mut config = minimal_config();
        config.http.bind = "127.0.0.1:0".to_string();
        let (_addr, _state) = init_state(&config, Some(&dir)).expect("state");

        let persisted = load_state(&state_path).expect("load state").expect("state");
        assert_eq!(persisted.current_version, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_serve_from_config_abort() {
        let mut config = minimal_config();
        config.http.bind = "127.0.0.1:0".to_string(); // Random port
        let dir = std::env::temp_dir().join(format!(
            "relay_serve_abort_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let data_dir = dir.clone();
        let handle =
            tokio::spawn(async move { super::serve_from_config(&config, Some(&data_dir)).await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        handle.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
