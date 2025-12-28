pub mod fs;

use anyhow::{Result, anyhow};
use pavis_core::RuntimeConfig;

/// Orchestrates the loading of a configuration file.
///
/// 1. Reads and deserializes the binary file (Boundary Layer).
/// 2. Validates the runtime configuration (Domain Layer).
pub fn load_file(path: &str) -> Result<RuntimeConfig> {
    if !path.ends_with(".pvs") {
        return Err(anyhow!(
            "Only .pvs configuration files are supported. Path: {}",
            path
        ));
    }

    let config = fs::read_pvs_file(path)?;

    Ok(config)
}
