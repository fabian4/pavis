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
        Ok(CheckedArtifact(artifact))
    }

    fn compile(&self, checked: &CheckedArtifact) -> Result<RuntimeConfig, CodecError> {
        let mut config: SerdeConfig = parse_with_format(self.format, &checked.0.bytes)
            .map_err(|err| CodecError::Compile(anyhow::anyhow!("Failed to parse: {err}")))?;
        config
            .validate()
            .map_err(|err| CodecError::Compile(anyhow::anyhow!("Failed to validate: {err}")))?;
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
    use pavis_codec_api::Codec;
    use pavis_ingest_api::{Artifact, Format, SourceInfo};

    #[test]
    fn compile_surfaces_build_errors() {
        let yaml = r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: ""
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - path: "/"
        destinations:
          - upstream: ""
            weight: 1
"#;
        let artifact = Artifact::new(
            Bytes::from_static(yaml.as_bytes()),
            Format::Yaml,
            SourceInfo::unknown(),
        );
        let codec = SerdeCodec {
            format: SerdeFormat::Yaml,
        };
        let checked = codec.check(artifact).expect("checked");
        let err = codec.compile(&checked).expect_err("compile");
        let msg = err.to_string();
        let source = std::error::Error::source(&err)
            .map(|s| s.to_string())
            .unwrap_or_default();
        assert!(msg.contains("codec compile failed"));
        assert!(
            source.contains("Failed to build RuntimeConfig")
                || source.contains("EmptyUpstreamName"),
            "{source}"
        );
    }
}
