//! Serde-backed codec that implements the check → compile pipeline.
//! All source-specific defaults are applied during `compile` only.
//! Core semantic validation happens in `Codec::materialize`, not in this crate.

pub mod config;
pub mod serde_helpers;

use pavis_codec_api::{CheckedArtifact, Codec, CodecError};
use pavis_core::RuntimeConfig;
use pavis_ingest_api::{Artifact, Format};

use crate::config::types::SerdeConfig;
use crate::serde_helpers::parse_with_format;

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
        if artifact.bytes.is_empty() {
            return Err(CodecError::Check(anyhow::anyhow!(
                "Artifact payload is empty"
            )));
        }
        if artifact.format != self.format.ingest_format() {
            return Err(CodecError::Check(anyhow::anyhow!(
                "Unsupported format: {:?}",
                artifact.format
            )));
        }
        Ok(CheckedArtifact::new(artifact))
    }

    fn compile(&self, checked: &CheckedArtifact) -> Result<RuntimeConfig, CodecError> {
        let mut config: SerdeConfig = parse_with_format(self.format, &checked.artifact.bytes)
            .map_err(|err| CodecError::Compile(anyhow::anyhow!("Failed to parse: {err}")))?;
        config
            .validate()
            .map_err(|err| CodecError::Compile(anyhow::anyhow!("Failed to validate: {err}")))?;
        crate::config::structural(config).try_into().map_err(|err| {
            CodecError::Compile(anyhow::anyhow!("Failed to build RuntimeConfig: {err}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{SerdeCodec, SerdeFormat};
    use bytes::Bytes;
    use pavis_codec_api::{CheckedArtifact, Codec, CodecError};
    use pavis_ingest_api::{Artifact, Format, SourceInfo};

    #[test]
    fn check_empty_artifact_fails() {
        let codec = SerdeCodec {
            format: SerdeFormat::Yaml,
        };
        let artifact = Artifact::new(Bytes::new(), Format::Yaml, SourceInfo::unknown());
        let err = codec.check(artifact).unwrap_err();
        match err {
            CodecError::Check(e) => assert_eq!(e.to_string(), "Artifact payload is empty"),
            _ => panic!("wrong error type"),
        }
    }

    #[test]
    fn check_wrong_format_fails() {
        let codec = SerdeCodec {
            format: SerdeFormat::Yaml,
        };
        let artifact = Artifact::new(
            Bytes::from_static(b"{}"),
            Format::Json,
            SourceInfo::unknown(),
        );
        let err = codec.check(artifact).unwrap_err();
        match err {
            CodecError::Check(e) => assert!(e.to_string().contains("Unsupported format")),
            _ => panic!("wrong error type"),
        }
    }

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
}
