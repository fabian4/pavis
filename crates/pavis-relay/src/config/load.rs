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

#[cfg(test)]
mod tests {
    use super::{decode_value, load, normalize_root};
    use std::path::PathBuf;

    #[test]
    fn load_returns_error_on_missing_file() {
        let path = PathBuf::from("missing-relay-config.yaml");
        let err = load(&path).expect_err("missing file");
        assert!(err.to_string().contains("failed to read relay config"));
    }

    #[test]
    fn load_returns_error_on_invalid_yaml() {
        let path = std::env::temp_dir().join("pavis_relay_invalid.yaml");
        std::fs::write(&path, "relay: [").expect("write");

        let err = load(&path).expect_err("invalid yaml");
        assert!(err.to_string().contains("failed to parse relay config"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_returns_error_on_missing_env() {
        let path = std::env::temp_dir().join("pavis_relay_missing_env.yaml");
        std::fs::write(&path, "relay:\n  identity:\n    name: \"${MISSING}\"").expect("write");

        let err = load(&path).expect_err("missing env");
        assert!(
            err.to_string()
                .contains("failed to expand environment variables")
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn decode_value_fails_for_unmapped_root() {
        let err = decode_value(serde_yaml::Value::String("bad".to_string())).expect_err("decode");
        assert!(err.to_string().contains("failed to decode relay config"));
    }

    #[test]
    fn normalize_root_handles_non_mapping() {
        let value = serde_yaml::Value::String("plain".to_string());
        let normalized = normalize_root(value.clone()).expect("normalize");
        assert_eq!(normalized, value);
    }

    #[test]
    fn normalize_root_ignores_non_mapping_distribution() {
        let value: serde_yaml::Value = serde_yaml::from_str(
            r#"
relay:
  distribution: "string"
"#,
        )
        .expect("yaml");
        let normalized = normalize_root(value).expect("normalize");
        let map = normalized.as_mapping().expect("map");
        assert!(
            map.get(serde_yaml::Value::String("security".to_string()))
                .is_none()
        );
    }

    #[test]
    fn normalize_root_allows_non_mapping_relay() {
        let value: serde_yaml::Value = serde_yaml::from_str(
            r#"
relay: "string"
"#,
        )
        .expect("yaml");
        let normalized = normalize_root(value).expect("normalize");
        assert_eq!(normalized, serde_yaml::Value::String("string".to_string()));
    }
}
