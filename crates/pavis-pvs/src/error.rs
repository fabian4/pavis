use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PvsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config file too small ({actual} bytes, need at least {min})")]
    TooSmall { min: usize, actual: usize },
    #[error("Invalid magic bytes in .pvs file")]
    InvalidMagic,
    #[error("Version mismatch! File: {file}, expected: {expected}")]
    VersionMismatch { file: u32, expected: u32 },
    #[error("Unsupported or missing hash algorithm: {0}")]
    UnsupportedAlgorithm(u32),
    #[error("Checksum mismatch! The configuration may be corrupted or tampered with")]
    ChecksumMismatch,
    #[error("Binary integrity check failed: {0}")]
    CorruptArchive(String),
    #[error("Serialization failed: {0}")]
    Serialization(String),
    #[error("Invalid .pvs extension: {0}")]
    InvalidExtension(PathBuf),
}

pub type PvsResult<T> = Result<T, PvsError>;

impl From<std::convert::Infallible> for PvsError {
    fn from(_: std::convert::Infallible) -> Self {
        unreachable!("Infallible should never be converted to PvsError")
    }
}
