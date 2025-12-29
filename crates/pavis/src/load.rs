use pavis_core::{CoreValidationError, ValidatedRuntimeConfig};
use pavis_pvs::{PvsError, ValidatedLoadError};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeLoadError {
    #[error(transparent)]
    Pvs(#[from] PvsError),
    #[error(transparent)]
    Core(#[from] CoreValidationError),
}

pub type LoadResult<T> = Result<T, RuntimeLoadError>;

/// Orchestrates the loading of a configuration file.
///
/// 1. Reads and deserializes the binary file via `pavis-pvs`.
/// 2. Returns validated `RuntimeConfig` after semantic validation.
pub fn load_file(path: &str) -> LoadResult<ValidatedRuntimeConfig> {
    if !path.ends_with(".pvs") {
        return Err(RuntimeLoadError::Pvs(PvsError::InvalidExtension(
            PathBuf::from(path),
        )));
    }

    pavis_pvs::load_validated(path).map_err(|err| match err {
        ValidatedLoadError::Pvs(err) => RuntimeLoadError::Pvs(err),
        ValidatedLoadError::Semantic(err) => RuntimeLoadError::Core(err),
    })
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
