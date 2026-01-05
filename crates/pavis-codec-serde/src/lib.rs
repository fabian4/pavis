pub mod config;
pub mod serde_helpers;

use pavis_codec_api::{CheckedArtifact, Codec, CodecError};
use pavis_core::RuntimeConfig;
use pavis_ingest_api::{Artifact, Format, SourceInfo};

use crate::config::types::SerdeConfig;
use crate::serde_helpers::{emit_with_format, parse_with_format};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerdeFormat {
    Json,
    Yaml,
}

impl SerdeFormat {
    fn ingest_format(self) -> Format {
        match self {
            SerdeFormat::Json => Format::Json,
            SerdeFormat::Yaml => Format::Yaml,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SerdeCodec {
    pub format: SerdeFormat,
}

impl Codec for SerdeCodec {
    type Error = CodecError;

    fn check(&self, artifact: Artifact) -> Result<CheckedArtifact, CodecError> {
        let mut config: SerdeConfig = parse_with_format(self.format, &artifact.bytes)
            .map_err(|err| CodecError::Check(anyhow::anyhow!("Failed to parse: {err}")))?;
        config
            .validate()
            .map_err(|err| CodecError::Check(anyhow::anyhow!("Failed to validate: {err}")))?;
        Ok(CheckedArtifact::with_state(artifact, config))
    }

    fn compile(&self, checked: &CheckedArtifact) -> Result<RuntimeConfig, CodecError> {
        let config = checked
            .state
            .as_ref()
            .and_then(|s| s.downcast_ref::<SerdeConfig>())
            .cloned();

        let config = match config {
            Some(c) => c,
            None => {
                let mut c: SerdeConfig = parse_with_format(self.format, &checked.artifact.bytes)
                    .map_err(|err| {
                        CodecError::Compile(anyhow::anyhow!("Failed to parse: {err}"))
                    })?;
                c.validate().map_err(|err| {
                    CodecError::Compile(anyhow::anyhow!("Failed to validate: {err}"))
                })?;
                c
            }
        };

        let runtime = config.build().map_err(|err| {
            CodecError::Compile(anyhow::anyhow!("Failed to build RuntimeConfig: {err}"))
        })?;
        Ok(runtime)
    }

    fn pack(&self, cfg: &RuntimeConfig) -> Result<Artifact, CodecError> {
        let config: SerdeConfig = cfg.clone().into();
        let bytes = emit_with_format(self.format, &config)
            .map_err(|err| CodecError::Compile(anyhow::anyhow!("Failed to serialize: {err}")))?;
        Ok(Artifact::new(
            bytes.into(),
            self.format.ingest_format(),
            SourceInfo::unknown(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{SerdeCodec, SerdeFormat};
    use bytes::Bytes;
    use pavis_codec_api::{CheckedArtifact, Codec};
    use pavis_ingest_api::{Artifact, Format, SourceInfo};

    #[test]
    fn compile_handles_missing_state() {
        let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
"#;
        let artifact = Artifact::new(
            Bytes::from_static(yaml.as_bytes()),
            Format::Yaml,
            SourceInfo::unknown(),
        );
        let codec = SerdeCodec {
            format: SerdeFormat::Yaml,
        };
        // Compile directly without state populated by check
        let checked = CheckedArtifact::new(artifact);
        let config = codec.compile(&checked).expect("compile");
        assert_eq!(config.listeners[0].address.port(), 8080);
    }

    #[test]
    fn pack_success() {
        let config = pavis_core::RuntimeConfig {
            listeners: vec![],
            telemetry: pavis_core::Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: pavis_core::ServiceName("test".to_string()),
                metrics: pavis_core::Metrics::Disabled,
                access_log: pavis_core::AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            },
            upstreams: vec![],
            routes: vec![],
        };
        let codec = SerdeCodec {
            format: SerdeFormat::Json,
        };
        let artifact = codec.pack(&config).expect("pack");
        assert_eq!(artifact.format, Format::Json);
        assert!(!artifact.bytes.is_empty());
    }
}
