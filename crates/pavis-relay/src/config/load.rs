use anyhow::{Context, Result};
use std::path::Path;

use super::env::expand_env;
use super::types::RelayConfig;

pub fn load(path: &Path) -> Result<RelayConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read relay config: {}", path.display()))?;
    let mut value: serde_yaml::Value = serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse relay config: {}", path.display()))?;
    expand_env(&mut value, &|k| std::env::var(k))
        .context("failed to expand environment variables")?;
    decode_value(value)
}

pub(super) fn decode_value(value: serde_yaml::Value) -> Result<RelayConfig> {
    let value = normalize_root(value)?;
    serde_yaml::from_value(value).context("failed to decode relay config")
}

fn normalize_root(mut value: serde_yaml::Value) -> Result<serde_yaml::Value> {
    let relay_key = serde_yaml::Value::String("relay".to_string());
    let distribution_key = serde_yaml::Value::String("distribution".to_string());
    let security_key = serde_yaml::Value::String("security".to_string());

    match &mut value {
        serde_yaml::Value::Mapping(map) => {
            if let Some(relay_value) = map.remove(&relay_key) {
                value = relay_value;
            }
        }
        _ => return Ok(value),
    }

    match &mut value {
        serde_yaml::Value::Mapping(map) => {
            if !map.contains_key(&security_key) {
                let nested_security =
                    map.get_mut(&distribution_key)
                        .and_then(|distribution| match distribution {
                            serde_yaml::Value::Mapping(dist_map) => dist_map.remove(&security_key),
                            _ => None,
                        });
                if let Some(security) = nested_security {
                    map.insert(security_key, security);
                }
            }
        }
        _ => return Ok(value),
    }

    Ok(value)
}
