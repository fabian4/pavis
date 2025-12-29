use anyhow::{Context, Result};
use pavis_codec_api::Codec;
use pavis_codec_serde::{SerdeCodec, SerdeFormat};
use pavis_core::{self as binary};
use pavis_ingest_api::{Artifact, Format, SourceInfo};

pub fn parse_runtime_from_bytes(
    format: SerdeFormat,
    bytes: &[u8],
) -> Result<binary::RuntimeConfig> {
    let ingest_format = match format {
        SerdeFormat::Yaml => Format::Yaml,
        SerdeFormat::Json => Format::Json,
    };
    let env = Artifact::new(bytes.to_vec().into(), ingest_format, SourceInfo::unknown());
    let codec = SerdeCodec { format };
    let validated = codec.materialize(env).context("Failed to decode config")?;
    Ok(validated.into_inner())
}
