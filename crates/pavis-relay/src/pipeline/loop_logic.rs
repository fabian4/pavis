use super::artifact::handle_artifact;
use super::backoff::Backoff;
use crate::codec::BoxedCodec;
use crate::config::PipelineOptions;
use crate::ingest::BoxedIngest;
use crate::runtime::RelayRuntimeState;
use anyhow::Result;
use futures_util::StreamExt;
use tracing::{debug, error, info, warn};

pub async fn start_pipeline(
    label: String,
    ingest: BoxedIngest,
    codec: BoxedCodec,
    state: RelayRuntimeState,
    options: PipelineOptions,
) -> Result<()> {
    tokio::spawn(async move {
        if let Err(e) = run_pipeline(label, ingest, codec, state, options).await {
            error!("Pipeline stopped with error: {}", e);
        }
    });

    Ok(())
}

pub async fn run_pipeline(
    label: String,
    mut ingest: BoxedIngest,
    codec: BoxedCodec,
    state: RelayRuntimeState,
    options: PipelineOptions,
) -> Result<()> {
    debug!("Spawning pipeline loop for: {}", label);

    let max_in_flight = options.max_in_flight.max(1);
    let mut restart_backoff = Backoff::new(options.restart_backoff);

    loop {
        let stream = match ingest.stream().await {
            Ok(stream) => stream,
            Err(err) => {
                warn!("[{}] Failed to start ingest stream: {}", label, err);
                let delay = restart_backoff.next_delay();
                tokio::time::sleep(delay).await;
                continue;
            }
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
