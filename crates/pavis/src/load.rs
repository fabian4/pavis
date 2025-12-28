pub mod fs;

use pavis_core::RuntimeConfig;

#[derive(Debug)]
pub enum LoadError {
    InvalidExtension(String),
    Io(std::io::Error),
    InvalidMagic,
    VersionMismatch { file: u32, expected: u32 },
    UnsupportedAlgorithm(u32),
    ChecksumMismatch,
    CorruptArchive(String),
    TooSmall { min: usize, actual: usize },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::InvalidExtension(p) => write!(f, "Only .pvs files are supported: {}", p),
            LoadError::Io(e) => write!(f, "I/O error: {}", e),
            LoadError::InvalidMagic => write!(f, "Invalid magic bytes in .pvs file"),
            LoadError::VersionMismatch { file, expected } => write!(
                f,
                "Version mismatch! File: {}, Runtime expects: {}. Recompile config.",
                file, expected
            ),
            LoadError::UnsupportedAlgorithm(id) => {
                write!(f, "Unsupported or missing hash algorithm: {}", id)
            }
            LoadError::ChecksumMismatch => {
                write!(
                    f,
                    "Checksum mismatch! The configuration may be corrupted or tampered with."
                )
            }
            LoadError::CorruptArchive(e) => write!(f, "Binary integrity check failed: {}", e),
            LoadError::TooSmall { min, actual } => {
                write!(
                    f,
                    "Config file too small ({} bytes, need at least {})",
                    actual, min
                )
            }
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::convert::Infallible> for LoadError {
    fn from(_: std::convert::Infallible) -> Self {
        unreachable!("Infallible should never be converted to LoadError")
    }
}

pub type LoadResult<T> = Result<T, LoadError>;

/// Orchestrates the loading of a configuration file.
///
/// 1. Reads and deserializes the binary file (Boundary Layer).
/// 2. Returns owned `RuntimeConfig` without semantic validation.
pub fn load_file(path: &str) -> LoadResult<RuntimeConfig> {
    if !path.ends_with(".pvs") {
        return Err(LoadError::InvalidExtension(path.to_string()));
    }

    let config = fs::read_pvs_file(path)?;

    Ok(config)
}
