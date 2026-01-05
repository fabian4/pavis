use pavis_codec_api::{Codec, CodecError};

pub type BoxedCodec = Box<dyn Codec<Error = CodecError> + Send + Sync>;
