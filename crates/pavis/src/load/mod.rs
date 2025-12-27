pub mod adapter;
pub mod fs;

use anyhow::{Context, Result, anyhow};
use pavis_core::config::ValidatedConfig;

/// Orchestrates the loading of a configuration file.
///
/// 1. Reads and deserializes the binary file (Boundary Layer).
/// 2. Converts the binary struct to the runtime DTO (Adapter Layer).
/// 3. Validates the runtime configuration (Domain Layer).
pub fn load_file(path: &str) -> Result<ValidatedConfig> {
    if !path.ends_with(".pvs") {
        return Err(anyhow!(
            "Only .pvs configuration files are supported. Path: {}",
            path
        ));
    }

    let binary_config = fs::read_pvs_file(path)?;

    let config = adapter::to_runtime_config(binary_config);

    config.validate().context("Config validation failed")
}
