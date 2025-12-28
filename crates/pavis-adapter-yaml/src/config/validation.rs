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

#[cfg(test)]
mod tests {
    use super::super::YamlConfig;

    #[test]
    fn validate_rejects_non_string_retry_on_values() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
        retry:
          attempts: 2
          per_try_timeout: "1s"
          retry_on: [1]
"#;
        let mut config = YamlConfig::parse_str(yaml).expect("parse config");
        let err = super::validate(&mut config).expect_err("validate should fail");
        assert!(
            err.to_string()
                .contains("retry.retry_on entries must be strings"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_allows_string_retry_on_values() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
        retry:
          attempts: 2
          per_try_timeout: "1s"
          retry_on: ["5xx", "connect-failure"]
"#;
        let mut config = YamlConfig::parse_str(yaml).expect("parse config");
        super::validate(&mut config).expect("validate should succeed");
    }
}
