use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};

use crate::SerdeFormat;

pub fn parse_with_format<T: DeserializeOwned>(format: SerdeFormat, bytes: &[u8]) -> Result<T> {
    let content = std::str::from_utf8(bytes).context("Config bytes must be UTF-8")?;
    match format {
        SerdeFormat::Json => serde_json::from_str(content).map_err(Into::into),
        SerdeFormat::Yaml => serde_yaml::from_str(content).map_err(Into::into),
    }
}

pub fn emit_with_format<T: Serialize>(format: SerdeFormat, value: &T) -> Result<Vec<u8>> {
    let out = match format {
        SerdeFormat::Json => serde_json::to_string_pretty(value)?,
        SerdeFormat::Yaml => serde_yaml::to_string(value)?,
    };
    Ok(out.into_bytes())
}
