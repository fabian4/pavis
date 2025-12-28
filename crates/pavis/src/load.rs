use pavis_core::RuntimeConfig;
use pavis_pvs::{PvsError, PvsResult};
use std::path::PathBuf;

pub type LoadError = PvsError;
pub type LoadResult<T> = PvsResult<T>;

/// Orchestrates the loading of a configuration file.
///
/// 1. Reads and deserializes the binary file via `pavis-pvs`.
/// 2. Returns owned `RuntimeConfig` without semantic validation.
pub fn load_file(path: &str) -> LoadResult<RuntimeConfig> {
    if !path.ends_with(".pvs") {
        return Err(PvsError::InvalidExtension(PathBuf::from(path)));
    }

    pavis_pvs::load(path)
}
