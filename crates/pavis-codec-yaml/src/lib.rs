pub mod config;

use pavis_codec_api::{CheckedArtifact, Codec, CodecError};
use pavis_core::{RuntimeConfig, ValidatedRuntimeConfig};
use pavis_ingest_api::{Artifact, Format, SourceInfo};

use crate::config::types::YamlConfig;

#[derive(Debug, Default)]
pub struct YamlCodec;

impl Codec for YamlCodec {
    type Error = CodecError;

    fn check(&self, artifact: Artifact) -> Result<CheckedArtifact, CodecError> {
        let mut config = YamlConfig::parse_bytes(&artifact.bytes)
            .map_err(|err| CodecError::Check(anyhow::anyhow!("Failed to parse YAML: {err}")))?;
        config
            .validate()
            .map_err(|err| CodecError::Check(anyhow::anyhow!("Failed to validate YAML: {err}")))?;
        Ok(CheckedArtifact(artifact))
    }

    fn compile(&self, checked: &CheckedArtifact) -> Result<RuntimeConfig, CodecError> {
        let mut config = YamlConfig::parse_bytes(&checked.0.bytes)
            .map_err(|err| CodecError::Compile(anyhow::anyhow!("Failed to parse YAML: {err}")))?;
        config.validate().map_err(|err| {
            CodecError::Compile(anyhow::anyhow!("Failed to validate YAML: {err}"))
        })?;
        let runtime = config.build().map_err(|err| {
            CodecError::Compile(anyhow::anyhow!("Failed to build RuntimeConfig: {err}"))
        })?;
        Ok(runtime)
    }

    fn decompile(&self, cfg: &RuntimeConfig) -> Result<Artifact, CodecError> {
        let yaml_config: YamlConfig = cfg.clone().into();
        let yaml = serde_yaml::to_string(&yaml_config).map_err(|err| {
            CodecError::Compile(anyhow::anyhow!("Failed to serialize YAML: {err}"))
        })?;
        Ok(Artifact::new(
            yaml.into(),
            Format::Yaml,
            SourceInfo::unknown(),
        ))
    }

    fn materialize(&self, art: Artifact) -> Result<ValidatedRuntimeConfig, CodecError> {
        let checked = self.check(art)?;
        let cfg = self.compile(&checked)?;
        pavis_core::validate_runtime(cfg).map_err(CodecError::Core)
    }
}
