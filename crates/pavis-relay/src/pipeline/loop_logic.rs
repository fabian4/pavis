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
    ingest: BoxedIngest,
    codec: BoxedCodec,
    state: RelayRuntimeState,
    options: PipelineOptions,
) -> Result<()> {
    run_pipeline_with_shutdown(label, ingest, codec, state, options, None).await
}

pub async fn run_pipeline_with_shutdown(
    label: String,
    mut ingest: BoxedIngest,
    codec: BoxedCodec,
    state: RelayRuntimeState,
    options: PipelineOptions,
    mut shutdown_rx: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<()> {
    debug!("Spawning pipeline loop for: {}", label);

    let max_in_flight = options.max_in_flight.max(1);
    let mut restart_backoff = Backoff::new(options.restart_backoff);

    loop {
        if let Some(rx) = &mut shutdown_rx
            && *rx.borrow()
        {
            break;
        }

        let stream = match ingest.stream().await {
            Ok(stream) => stream,
            Err(err) => {
                warn!("[{}] Failed to start ingest stream: {}", label, err);
                let delay = restart_backoff.next_delay();
                tokio::select! {
                    _ = tokio::time::sleep(delay) => { continue; }
                    _ = async {
                        if let Some(rx) = &mut shutdown_rx {
                            let _ = rx.changed().await;
                        } else {
                            futures_util::future::pending::<()>().await;
                        }
                    } => { break; }
                }
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

        loop {
            tokio::select! {
                outcome = processing.next() => {
                    match outcome {
                        Some(result) => {
                            if let Err(err) = result {
                                warn!("[{}] Pipeline artifact failed: {}", label, err);
                            }
                        }
                        None => {
                            warn!("[{}] Ingest stream ended; restarting", label);
                            break;
                        }
                    }
                }
                _ = async {
                    if let Some(rx) = &mut shutdown_rx {
                        let _ = rx.changed().await;
                    } else {
                        futures_util::future::pending::<()>().await;
                    }
                } => {
                    return Ok(());
                }
            }
        }

        let delay = restart_backoff.next_delay();
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = async {
                if let Some(rx) = &mut shutdown_rx {
                    let _ = rx.changed().await;
                } else {
                    futures_util::future::pending::<()>().await;
                }
            } => { break; }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PipelineConfig;
    use bytes::Bytes;
    use pavis_codec_api::{CheckedArtifact, Codec, CodecError};
    use pavis_core::RuntimeConfig;
    use pavis_ingest_api::{Artifact, Format, Ingest, IngestError, SourceInfo};
    use tokio::sync::watch;

    struct MockIngest {
        fail_stream: bool,
    }
    #[async_trait::async_trait]
    impl Ingest for MockIngest {
        type Stream = futures_util::stream::Iter<std::vec::IntoIter<Result<Artifact, IngestError>>>;
        async fn stream(&mut self) -> Result<Self::Stream, IngestError> {
            if self.fail_stream {
                return Err(IngestError::Io(anyhow::anyhow!("fail")));
            }
            let art = Artifact::new("test".into(), Format::Yaml, SourceInfo::unknown());
            Ok(futures_util::stream::iter(vec![Ok(art)]))
        }
    }

    struct MockCodec;
    impl Codec for MockCodec {
        type Error = CodecError;
        fn check(&self, art: Artifact) -> Result<CheckedArtifact, Self::Error> {
            Ok(CheckedArtifact::new(art))
        }
        fn compile(&self, _checked: &CheckedArtifact) -> Result<RuntimeConfig, Self::Error> {
            // Minimal valid config needed by handle_artifact
            Ok(pavis_core::RuntimeConfigBuilder::new()
                .telemetry(pavis_core::Telemetry {
                    level: pavis_core::LogLevel::Info,
                    pingora: pavis_core::LogLevel::Info,
                    service_name: pavis_core::ServiceName("test".into()),
                    metrics: pavis_core::Metrics::Disabled,
                    access_log: pavis_core::AccessLogPolicy::Disabled,
                    tracing: pavis_core::TracingPolicy::Disabled,
                })
                .add_listener(
                    pavis_core::ListenerBuilder::new()
                        .name(pavis_core::ListenerName("test".into()))
                        .address("127.0.0.1:0".parse().unwrap())
                        .workers(pavis_core::WorkerCount::Auto)
                        .tls(pavis_core::TlsConfig::Disabled)
                        .build()
                        .unwrap(),
                )
                .build()
                .unwrap())
        }
    }

    #[tokio::test]
    async fn test_pipeline_shutdown() {
        let label = "test".to_string();
        let ingest = crate::ingest::boxed_ingest(MockIngest { fail_stream: false });
        let codec = Box::new(MockCodec);
        let state = RelayRuntimeState::new(0, Bytes::new()).unwrap();
        let options = PipelineOptions::from_config(&PipelineConfig::default());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let handle = tokio::spawn(async move {
            run_pipeline_with_shutdown(label, ingest, codec, state, options, Some(shutdown_rx))
                .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        shutdown_tx.send(true).unwrap();

        let res = tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_pipeline_ingest_failure_backoff() {
        let label = "test".to_string();
        let ingest = crate::ingest::boxed_ingest(MockIngest { fail_stream: true });
        let codec = Box::new(MockCodec);
        let state = RelayRuntimeState::new(0, Bytes::new()).unwrap();
        let mut options = PipelineOptions::from_config(&PipelineConfig::default());
        options.restart_backoff.base_delay = std::time::Duration::from_millis(10);

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let handle = tokio::spawn(async move {
            run_pipeline_with_shutdown(label, ingest, codec, state, options, Some(shutdown_rx))
                .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        shutdown_tx.send(true).unwrap();

        let res = tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();
        assert!(res.is_ok());
    }
}
