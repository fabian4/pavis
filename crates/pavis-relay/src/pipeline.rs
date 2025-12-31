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
        "[{}] Received artifact: format={:?} source={}",
        label, artifact.format, source
    );

    let validated_config = match codec {
        CodecImpl::Serde(c) => c
            .materialize(artifact, compaction_level(compaction))
            .with_context(|| format!("materialize config for source {}", source))?,
    };

    debug!("[{}] Materialized and validated config", label);

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
}
