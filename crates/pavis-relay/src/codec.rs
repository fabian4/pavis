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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_codec_creates_serde_yaml() {
        let config = PipelineConfig::default();
        let codec = create_codec(&config).expect("create codec");
        match codec {
            Some(CodecImpl::Serde(c)) => {
                assert!(matches!(c.format, SerdeFormat::Yaml));
            }
            None => panic!("expected codec"),
        }
    }
}
