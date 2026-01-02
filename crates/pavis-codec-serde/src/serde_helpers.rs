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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct MockConfig {
        name: String,
        enabled: bool,
    }

    #[test]
    fn test_json_round_trip() {
        let config = MockConfig {
            name: "test".to_string(),
            enabled: true,
        };
        let bytes = emit_with_format(SerdeFormat::Json, &config).expect("emit json");
        let content = std::str::from_utf8(&bytes).expect("utf8");
        assert!(content.contains("\"name\": \"test\""));
        assert!(content.contains("\"enabled\": true"));

        let parsed: MockConfig = parse_with_format(SerdeFormat::Json, &bytes).expect("parse json");
        assert_eq!(parsed, config);
    }

    #[test]
    fn test_yaml_round_trip() {
        let config = MockConfig {
            name: "test".to_string(),
            enabled: true,
        };
        let bytes = emit_with_format(SerdeFormat::Yaml, &config).expect("emit yaml");
        let content = std::str::from_utf8(&bytes).expect("utf8");
        assert!(content.contains("name: test"));
        assert!(content.contains("enabled: true"));

        let parsed: MockConfig = parse_with_format(SerdeFormat::Yaml, &bytes).expect("parse yaml");
        assert_eq!(parsed, config);
    }

    #[test]
    fn test_parse_invalid_utf8() {
        let bytes = vec![0, 159, 146, 150]; // Invalid UTF-8
        let result: Result<MockConfig> = parse_with_format(SerdeFormat::Json, &bytes);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Config bytes must be UTF-8")
        );
    }
}
