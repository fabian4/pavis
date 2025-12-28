use super::YamlConfig;
use anyhow::{Result, bail};

/// Source-specific validation placeholder.
/// Canonical semantic validation now lives in `pavis-core::validate_runtime_config`.
pub fn validate(config: &mut YamlConfig) -> Result<()> {
    for vhost in &config.routes {
        for route in &vhost.paths {
            if let Some(retry) = &route.retry {
                for value in &retry.retry_on {
                    if !value.is_string() {
                        bail!(
                            "retry.retry_on entries must be strings (host: {}, path: {})",
                            vhost.host,
                            route.path
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
