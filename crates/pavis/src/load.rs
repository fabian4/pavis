use pavis_core::{RuntimeConfig, ValidatedRuntimeConfig};
use pavis_pvs::PvsError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeLoadError {
    #[error(transparent)]
    Pvs(#[from] PvsError),
}

pub type LoadResult<T> = Result<T, RuntimeLoadError>;

/// Orchestrates the loading of a configuration file.
///
/// 1. Reads and deserializes the binary file via `pavis-pvs`.
/// 2. Returns a `RuntimeConfig` trusted to be semantically validated by the producer.
pub fn load_file(path: &str) -> LoadResult<ValidatedRuntimeConfig> {
    if !path.ends_with(".pvs") {
        return Err(RuntimeLoadError::Pvs(PvsError::InvalidExtension(
            PathBuf::from(path),
        )));
    }

    let config = pavis_pvs::load(path)?;
    Ok(assume_validated(config))
}

pub(crate) fn assume_validated(config: RuntimeConfig) -> ValidatedRuntimeConfig {
    // SAFETY: `.pvs` artifacts are produced after canonical validation; runtime does not
    // perform semantic inference or mutation after loading.
    unsafe { ValidatedRuntimeConfig::from_trusted(config) }
}

#[cfg(test)]
mod tests {
    use super::load_file;
    use pavis_pvs::PvsError;

    #[test]
    fn load_file_rejects_non_pvs_extension() {
        let err = load_file("config.yaml").expect_err("expected invalid extension error");
        assert!(matches!(
            err,
            super::RuntimeLoadError::Pvs(PvsError::InvalidExtension(_))
        ));
    }
}
