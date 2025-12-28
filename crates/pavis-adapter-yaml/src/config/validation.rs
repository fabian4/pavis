use super::YamlConfig;
use anyhow::Result;

/// Source-specific validation placeholder.
/// Canonical semantic validation now lives in `pavis-core::validate_runtime_config`.
pub fn validate(_config: &mut YamlConfig) -> Result<()> {
    Ok(())
}
