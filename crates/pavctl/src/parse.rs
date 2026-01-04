use anyhow::{Context, Result};
use pavis_codec_api::{Codec, CompactionLevel};
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
    let validated = codec
        .materialize(env, CompactionLevel::Off)
        .context("Failed to decode config")?;
    Ok(validated.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_runtime_handles_yaml() {
        let input = b"listeners:\n  - name: default\n    address: 127.0.0.1:8080";
        let config = parse_runtime_from_bytes(SerdeFormat::Yaml, input).expect("yaml");
        assert_eq!(config.listeners[0].address.port(), 8080);
    }

    #[test]
    fn parse_runtime_handles_json() {
        let input = br#"{
            "listeners": [{
                "name": "default",
                "address": "127.0.0.1:9090"
            }]
        }"#;
        let config = parse_runtime_from_bytes(SerdeFormat::Json, input).expect("json");
        assert_eq!(config.listeners[0].address.port(), 9090);
    }
}
