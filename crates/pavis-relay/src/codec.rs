use crate::config::PipelineConfig;
use anyhow::Result;
use pavis_codec_serde::{SerdeCodec, SerdeFormat};

pub enum CodecImpl {
    Serde(SerdeCodec),
}

pub fn create_codec(config: &PipelineConfig) -> Result<Option<CodecImpl>> {
    match config.codec.kind {
        crate::config::CodecKind::Serde => Ok(Some(CodecImpl::Serde(SerdeCodec {
            format: SerdeFormat::Yaml,
        }))),
    }
}
