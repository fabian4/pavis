use crate::codec::{CodecImpl, create_codec};
use crate::config::{
    BackoffConfig, PipelineCompaction, PipelineConfig, PipelineOptions, RetryPolicy,
};
use crate::ingest::{IngestImpl, create_ingest};
use crate::state::RelayState;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use pavis_codec_api::{Codec, CompactionLevel};
use pavis_ingest_api::{Artifact, Ingest, IngestError};
use std::time::Duration;
use tracing::{debug, error, info, warn};

pub async fn start_pipeline(config: &PipelineConfig, state: RelayState) -> Result<()> {
    let ingest = match create_ingest(config)? {
        Some(i) => i,
        None => return Ok(()),
    };

    let codec = match create_codec(config)? {
        Some(c) => c,
        None => return Ok(()),
    };

    let label = format!("{:?}-{:?}", config.ingest.source, config.codec);
    let options = PipelineOptions::from_config(config);

    tokio::spawn(async move {
        if let Err(e) = run_pipeline(label, ingest, codec, state, options).await {
            error!("Pipeline stopped with error: {}", e);
        }
    });

    Ok(())
}

async fn run_pipeline(
    label: String,
    mut ingest: IngestImpl,
    codec: CodecImpl,
    state: RelayState,
    options: PipelineOptions,
) -> Result<()> {
    debug!("Spawning pipeline loop for: {}", label);

    let max_in_flight = options.max_in_flight.max(1);
    let mut restart_backoff = Backoff::new(options.restart_backoff);

    loop {
        let stream = match ingest {
            IngestImpl::File(ref mut i) => match i.stream().await {
                Ok(stream) => stream,
                Err(err) => {
                    warn!("[{}] Failed to start ingest stream: {}", label, err);
                    let delay = restart_backoff.next_delay();
                    tokio::time::sleep(delay).await;
                    continue;
                }
            },
        };

        restart_backoff.reset();
        info!("Started pipeline: {}", label);

        let mut processing = stream
            .map(|result| {
                handle_artifact(
                    &label,
                    result,
                    &codec,
                    &state,
                    options.compaction,
                    options.publish_retry,
                )
            })
            .buffer_unordered(max_in_flight);

        while let Some(outcome) = processing.next().await {
            if let Err(err) = outcome {
                warn!("[{}] Pipeline artifact failed: {}", label, err);
            }
        }

        warn!("[{}] Ingest stream ended; restarting", label);
        let delay = restart_backoff.next_delay();
        tokio::time::sleep(delay).await;
    }
}

async fn handle_artifact(
    label: &str,
    result: Result<Artifact, IngestError>,
    codec: &CodecImpl,
    state: &RelayState,
    compaction: PipelineCompaction,
    publish_retry: RetryPolicy,
) -> Result<()> {
    let artifact = result.context("ingest stream error")?;
    let source = artifact.source.name.clone();

    debug!(
        "[{}] Received artifact: format={:?} source={} bytes={}",
        label,
        artifact.format,
        source,
        artifact.bytes.len()
    );

    let validated_config = match codec {
        CodecImpl::Serde(c) => c
            .materialize(artifact, compaction_level(compaction))
            .with_context(|| format!("materialize config for source {}", source))?,
    };

    debug!(
        "[{}] Materialized and validated config: routes={}, upstreams={}",
        label,
        validated_config.routes.len(),
        validated_config.upstreams.len()
    );

    let version = publish_with_retry(
        state,
        validated_config.as_ref(),
        publish_retry,
        label,
        &source,
    )
    .await?;

    info!(
        "[{}] Automatically updated config to version: {}",
        label, version
    );

    Ok(())
}

struct Backoff {
    config: BackoffConfig,
    next: Duration,
}

impl Backoff {
    fn new(config: BackoffConfig) -> Self {
        Self {
            config,
            next: config.base_delay,
        }
    }

    fn reset(&mut self) {
        self.next = self.config.base_delay;
    }

    fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        let next = delay.saturating_mul(2);
        self.next = std::cmp::min(next, self.config.max_delay);
        delay
    }
}

async fn publish_with_retry(
    state: &RelayState,
    config: &pavis_core::RuntimeConfig,
    policy: RetryPolicy,
    label: &str,
    source: &str,
) -> Result<u64> {
    let mut attempt = 0;
    let mut delay = policy.base_delay;

    loop {
        attempt += 1;
        match state.publish_config(config).await {
            Ok(version) => return Ok(version),
            Err(err) if attempt <= policy.max_attempts => {
                warn!(
                    "[{}] Publish failed (attempt {} of {}) for source {}: {}",
                    label, attempt, policy.max_attempts, source, err
                );
                tokio::time::sleep(delay).await;
                delay = std::cmp::min(delay.saturating_mul(2), policy.max_delay);
            }
            Err(err) => {
                return Err(anyhow::anyhow!(err)).context("publish config after retries");
            }
        }
    }
}

fn compaction_level(level: PipelineCompaction) -> CompactionLevel {
    match level {
        PipelineCompaction::Off => CompactionLevel::Off,
        PipelineCompaction::Trim => CompactionLevel::Trim,
        PipelineCompaction::Prune => CompactionLevel::Prune,
    }
}

#[cfg(test)]

mod tests {

    use super::*;

    #[test]

    fn backoff_expands_exponentially_and_clamps() {
        let config = BackoffConfig {
            base_delay: Duration::from_millis(100),

            max_delay: Duration::from_millis(400),
        };

        let mut backoff = Backoff::new(config);

        // First call: base delay

        assert_eq!(backoff.next_delay(), Duration::from_millis(100));

        // Second call: 2x base (200)

        assert_eq!(backoff.next_delay(), Duration::from_millis(200));

        // Third call: 4x base (400) - hits max

        assert_eq!(backoff.next_delay(), Duration::from_millis(400));

        // Fourth call: stays at max

        assert_eq!(backoff.next_delay(), Duration::from_millis(400));

        // Reset

        backoff.reset();

        assert_eq!(backoff.next_delay(), Duration::from_millis(100));
    }

    #[test]

    fn options_from_config_maps_correctly() {
        let mut config = PipelineConfig::default();

        config.runtime.max_in_flight = 10;

        config.codec.mode.compaction = PipelineCompaction::Prune;

        config.runtime.restart_backoff.min = 100;

        config.runtime.restart_backoff.max = 200;

        config.runtime.publish_retry.max = 3;

        config.runtime.publish_retry.backoff.min = 50;

        config.runtime.publish_retry.backoff.max = 150;

        let options = PipelineOptions::from_config(&config);

        assert_eq!(options.max_in_flight, 10);

        assert!(matches!(options.compaction, PipelineCompaction::Prune));

        assert_eq!(
            options.restart_backoff.base_delay,
            Duration::from_millis(100)
        );

        assert_eq!(
            options.restart_backoff.max_delay,
            Duration::from_millis(200)
        );

        assert_eq!(options.publish_retry.max_attempts, 3);

        assert_eq!(options.publish_retry.base_delay, Duration::from_millis(50));

        assert_eq!(options.publish_retry.max_delay, Duration::from_millis(150));
    }

    #[test]
    fn compaction_level_mapping() {
        assert!(matches!(
            compaction_level(PipelineCompaction::Off),
            CompactionLevel::Off
        ));

        assert!(matches!(
            compaction_level(PipelineCompaction::Trim),
            CompactionLevel::Trim
        ));

        assert!(matches!(
            compaction_level(PipelineCompaction::Prune),
            CompactionLevel::Prune
        ));
    }

    #[tokio::test]
    async fn handle_artifact_processes_valid_artifact() {
        use axum::body::Bytes;
        let state = RelayState::new(0, Bytes::new()).expect("state");
        let codec = CodecImpl::Serde(pavis_codec_serde::SerdeCodec {
            format: pavis_codec_serde::SerdeFormat::Yaml,
        });

        let valid_yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: disabled
upstreams: []
routes: []
"#;
        let artifact = Artifact::new(
            Bytes::from(valid_yaml),
            pavis_ingest_api::Format::Yaml,
            pavis_ingest_api::SourceInfo::new("test-source"),
        );

        let result = handle_artifact(
            "test-pipeline",
            Ok(artifact),
            &codec,
            &state,
            PipelineCompaction::Off,
            RetryPolicy {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                max_delay: Duration::from_millis(1),
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(state.version().await, 1);
    }

    #[tokio::test]
    async fn handle_artifact_handles_invalid_artifact() {
        use axum::body::Bytes;
        let state = RelayState::new(0, Bytes::new()).expect("state");
        let codec = CodecImpl::Serde(pavis_codec_serde::SerdeCodec {
            format: pavis_codec_serde::SerdeFormat::Yaml,
        });

        let invalid_yaml = "not a valid yaml";
        let artifact = Artifact::new(
            Bytes::from(invalid_yaml),
            pavis_ingest_api::Format::Yaml,
            pavis_ingest_api::SourceInfo::new("test-source"),
        );

        let result = handle_artifact(
            "test-pipeline",
            Ok(artifact),
            &codec,
            &state,
            PipelineCompaction::Off,
            RetryPolicy {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                max_delay: Duration::from_millis(1),
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(state.version().await, 0);
    }

    #[tokio::test]
    async fn publish_with_retry_fails_eventually() {
        use axum::body::Bytes;
        // Create state with a very small size limit to force failure
        let mut options = crate::state::RelayOptions::default();
        options.max_pvs_bytes = 10; // Very small limit
        let state = RelayState::new_with_options(0, Bytes::new(), options).expect("state");

        let config = pavis_core::RuntimeConfig {
            server: pavis_core::ServerConfig {
                listen_addr: "0.0.0.0:8080".parse().unwrap(),
                worker_threads: None,
                tls: None,
            },
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

        // This config will exceed 10 bytes when serialized
        let policy = RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
        };

        let result =
            publish_with_retry(&state, &config, policy, "test-pipeline", "test-source").await;

        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        eprintln!("Actual error: {}", err);
        // Error should be about policy violation
        assert!(
            err.contains("pvs size"),
            "Error message '{}' did not contain 'pvs size'",
            err
        );
    }

    #[tokio::test]
    async fn test_pipeline_integration() {
        use crate::config::{CodecKind, IngestMode, IngestSource};
        use std::time::Duration;

        let dir = std::env::temp_dir().join("relay_pipeline_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.yaml");

        // Write initial valid config
        let initial_yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: disabled
upstreams: []
routes: []
"#;
        std::fs::write(&config_path, initial_yaml).unwrap();

        let mut config = PipelineConfig::default();
        config.ingest.source = IngestSource::File(crate::config::FileSourceConfig {
            path: config_path.to_string_lossy().to_string(),
        });
        config.ingest.mode = IngestMode {
            kind: "".to_string(),
            debounce: 10,
        };
        config.codec.kind = CodecKind::Serde;

        let mut options = crate::state::RelayOptions::default();
        // Disable persistence to simplify
        options.persistence.enabled = false;

        let state =
            RelayState::new_with_options(0, axum::body::Bytes::new(), options).expect("state");

        start_pipeline(&config, state.clone())
            .await
            .expect("start pipeline");

        // Wait for initial load
        let mut attempts = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if state.version().await > 0 {
                break;
            }
            attempts += 1;
            if attempts > 20 {
                panic!("timed out waiting for initial version");
            }
        }

        assert_eq!(state.version().await, 1);

        // Update file
        let update_yaml = r#"
server:
  listen_addr: "0.0.0.0:9090"
telemetry:
  access_log: disabled
upstreams: []
routes: []
"#;
        std::fs::write(&config_path, update_yaml).unwrap();

        // Wait for update
        let mut attempts = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if state.version().await > 1 {
                break;
            }
            attempts += 1;
            if attempts > 20 {
                panic!("timed out waiting for updated version");
            }
        }

        assert_eq!(state.version().await, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
