use pavis_codec_api::{CheckedArtifact, Codec, CodecError};
use pavis_core::RuntimeConfig;
use pavis_ingest_api::{Artifact, Format};

pub type BoxedCodec = Box<dyn Codec<Error = CodecError> + Send + Sync>;

#[cfg(feature = "codec-serde")]
#[derive(Debug, Default)]
pub struct AutoSerdeCodec;

#[cfg(feature = "codec-serde")]
impl AutoSerdeCodec {
    fn codec_for_format(format: Format) -> Result<pavis_codec_serde::SerdeCodec, CodecError> {
        let serde_format = match format {
            Format::Yaml => pavis_codec_serde::SerdeFormat::Yaml,
            Format::Json => pavis_codec_serde::SerdeFormat::Json,
            other => {
                return Err(CodecError::Check(anyhow::anyhow!(
                    "Unsupported format: {:?}",
                    other
                )));
            }
        };
        Ok(pavis_codec_serde::SerdeCodec {
            format: serde_format,
        })
    }
}

#[cfg(feature = "codec-serde")]
impl Codec for AutoSerdeCodec {
    type Error = CodecError;

    fn check(&self, artifact: Artifact) -> Result<CheckedArtifact, Self::Error> {
        Self::codec_for_format(artifact.format)?.check(artifact)
    }

    fn compile(&self, checked: &CheckedArtifact) -> Result<RuntimeConfig, Self::Error> {
        Self::codec_for_format(checked.artifact.format)?.compile(checked)
    }
}

#[cfg(all(test, feature = "codec-serde"))]
mod tests {
    use super::AutoSerdeCodec;
    use pavis_codec_api::{Codec, CompactionLevel};
    use pavis_ingest_api::{Artifact, Format, SourceInfo};
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../pavis-codec-serde/tests/fixtures")
            .join(name)
    }

    #[test]
    fn auto_codec_accepts_json_artifacts() {
        let codec = AutoSerdeCodec;
        let bytes = std::fs::read(fixture_path("minimal.json")).expect("read fixture");
        let artifact = Artifact::new(bytes.into(), Format::Json, SourceInfo::unknown());

        let validated = codec.materialize(artifact, CompactionLevel::Off);
        assert!(validated.is_ok());
    }
}
