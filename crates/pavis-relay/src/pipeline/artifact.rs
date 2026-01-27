use crate::codec::BoxedCodec;
use crate::config::{PipelineCompaction, RetryPolicy};
use crate::runtime::RelayRuntimeState;
use anyhow::{Context, Result};
use pavis_codec_api::{CodecError, CompactionLevel};
use pavis_ingest_api::{Artifact, IngestError};
use tracing::{debug, info, warn};

pub async fn handle_artifact(
    label: &str,
    result: Result<Artifact, IngestError>,
    codec: &BoxedCodec,
    state: &RelayRuntimeState,
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

    let validated_config = match codec.materialize(artifact, compaction_level(compaction)) {
        Ok(config) => config,
        Err(err) => {
            match &err {
                CodecError::Check(_) => {
                    warn!(
                        "[{}] Codec check failed for source {}: {}",
                        label, source, err
                    );
                }
                CodecError::Compile(_) => {
                    warn!(
                        "[{}] Codec compile failed for source {}: {}",
                        label, source, err
                    );
                }
                CodecError::Core(_) => {
                    warn!(
                        "[{}] Codec core validation failed for source {}: {}",
                        label, source, err
                    );
                }
            }
            return Err(anyhow::Error::new(err))
                .context(format!("materialize config for source {}", source));
        }
    };

    debug!("[{}] Materialized and validated config", label);

    let version =
        publish_with_retry(state, &validated_config, publish_retry, label, &source).await?;

    info!(
        "[{}] Automatically updated config to version: {}",
        label, version
    );

    Ok(())
}

async fn publish_with_retry(
    state: &RelayRuntimeState,
    config: &pavis_core::ValidatedRuntimeConfig,
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
