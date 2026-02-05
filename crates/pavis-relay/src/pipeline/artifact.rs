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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RetryPolicy;
    use crate::runtime::RelayRuntimeState;
    use axum::body::Bytes;
    use pavis_codec_api::{CheckedArtifact, Codec, CodecError, CompactionLevel};
    use pavis_core::{
        AccessLogPolicy, ListenerBuilder, ListenerName, LogLevel, Metrics, RuntimeConfig,
        RuntimeConfigBuilder, ServiceName, Telemetry, TracingPolicy,
    };
    use pavis_ingest_api::{Artifact, Format, SourceInfo};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;

    struct MockCodec;
    impl Codec for MockCodec {
        type Error = CodecError;
        fn materialize(
            &self,
            artifact: Artifact,
            _level: CompactionLevel,
        ) -> Result<pavis_core::ValidatedRuntimeConfig, Self::Error> {
            if *artifact.bytes == *b"fail" {
                return Err(CodecError::Check(anyhow::anyhow!("injected failure")));
            }
            if *artifact.bytes == *b"compile_fail" {
                return Err(CodecError::Compile(anyhow::anyhow!("injected failure")));
            }
            if *artifact.bytes == *b"core_fail" {
                return Err(CodecError::Core(
                    pavis_core::CoreValidationError::DuplicateUpstream(
                        "injected failure".to_string(),
                    ),
                ));
            }

            let listener = ListenerBuilder::new()
                .name(ListenerName("test".to_string()))
                .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
                .build()
                .unwrap();

            let config = RuntimeConfigBuilder::new()
                .telemetry(Telemetry {
                    level: LogLevel::Info,
                    pingora: LogLevel::Info,
                    service_name: ServiceName("test".to_string()),
                    metrics: Metrics::Disabled,
                    access_log: AccessLogPolicy::Disabled,
                    tracing: TracingPolicy::Disabled,
                })
                .add_listener(listener)
                .build()
                .unwrap();
            Ok(unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) })
        }
        fn check(&self, artifact: Artifact) -> Result<CheckedArtifact, Self::Error> {
            Ok(CheckedArtifact::new(artifact))
        }
        fn compile(&self, _artifact: &CheckedArtifact) -> Result<RuntimeConfig, Self::Error> {
            todo!()
        }
    }

    #[tokio::test]
    async fn test_handle_artifact_success() {
        let state = RelayRuntimeState::new(0, Bytes::new()).unwrap();
        let codec: BoxedCodec = Box::new(MockCodec);
        let artifact = Artifact::new(
            b"ok".to_vec().into(),
            Format::Yaml,
            SourceInfo {
                name: "test".into(),
                ..Default::default()
            },
        );

        let res = handle_artifact(
            "test-label",
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

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_handle_artifact_codec_fails() {
        let state = RelayRuntimeState::new(0, Bytes::new()).unwrap();
        let codec: BoxedCodec = Box::new(MockCodec);

        let failures = [
            b"fail".as_slice(),
            b"compile_fail".as_slice(),
            b"core_fail".as_slice(),
        ];
        for fail_bytes in failures {
            let artifact = Artifact::new(
                fail_bytes.to_vec().into(),
                Format::Yaml,
                SourceInfo {
                    name: "test".into(),
                    ..Default::default()
                },
            );

            let res = handle_artifact(
                "test-label",
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

            assert!(res.is_err());
        }
    }

    #[tokio::test]
    async fn test_publish_with_retry_success() {
        let state = RelayRuntimeState::new(0, Bytes::new()).unwrap();

        let listener = ListenerBuilder::new()
            .name(ListenerName("test".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
            .build()
            .unwrap();

        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("test".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .unwrap();
        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };

        let policy = RetryPolicy {
            max_attempts: 1,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
        };

        let res = publish_with_retry(&state, &validated, policy, "label", "source").await;
        assert!(res.is_ok());
    }
}
