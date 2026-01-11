use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PvsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config file too small ({actual} bytes, need at least {min})")]
    TooSmall { min: usize, actual: usize },
    #[error("Invalid magic bytes: expected {expected:?}, found {found:?}")]
    InvalidMagic { expected: String, found: String },
    #[error("Version mismatch! File: {file}, expected: {expected}")]
    VersionMismatch { file: u32, expected: u32 },
    #[error("Unsupported or missing hash algorithm: {0}")]
    UnsupportedAlgorithm(u32),
    #[error("Checksum mismatch! Expected: {expected}, Found: {found}")]
    ChecksumMismatch { expected: String, found: String },
    #[error("Binary integrity check failed: {0}")]
    CorruptArchive(String),
    #[error("Serialization failed: {0}")]
    Serialization(String),
    #[error("Invalid .pvs extension: {0}")]
    InvalidExtension(PathBuf),
    #[error("Header too short: expected {expected}, found {found}")]
    HeaderTooShort { expected: usize, found: usize },
    #[error("Payload too large: max {max} bytes, found {found} bytes")]
    PayloadTooLarge { max: usize, found: usize },
}

pub type PvsResult<T> = Result<T, PvsError>;

impl From<std::convert::Infallible> for PvsError {
    fn from(_: std::convert::Infallible) -> Self {
        unreachable!("Infallible should never be converted to PvsError")
    }
}

#[cfg(test)]
mod tests {
    use super::PvsError;

    #[test]
    fn invalid_extension_error_formats_path() {
        let err = PvsError::InvalidExtension("config.yaml".into());
        assert!(err.to_string().contains("config.yaml"));
    }
}
