// Codec-level validation tests
// These tests verify that pavis-codec-serde catches invalid configuration
// during YAML parsing and semantic validation, before reaching pavis-core.

use bytes::Bytes;
use pavis_codec_api::{Codec, CompactionLevel};
use pavis_codec_serde::{SerdeCodec, SerdeFormat};
use pavis_ingest_api::{Artifact, Format, SourceInfo};

fn compile_yaml(yaml: &str) -> Result<pavis_core::RuntimeConfig, pavis_codec_api::CodecError> {
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let artifact = Artifact::new(
        Bytes::from(yaml.as_bytes().to_vec()),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    let checked = codec.check(artifact)?;
    codec.compile(&checked)
}

fn materialize_yaml(
    yaml: &str,
) -> Result<pavis_core::ValidatedRuntimeConfig, pavis_codec_api::CodecError> {
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let artifact = Artifact::new(
        Bytes::from(yaml.as_bytes().to_vec()),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    codec.materialize(artifact, CompactionLevel::Off)
}

#[test]
fn test_invalid_regex_rejected() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher:
          path: !regex { path: "[" }
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let result = materialize_yaml(yaml);
    assert!(result.is_err(), "Should reject invalid regex");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("regex") || err.contains("InvalidRegex"),
        "Error should mention regex validation, got: {}",
        err
    );
}

#[test]
fn test_missing_upstream_reference_rejected() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        destinations:
          - upstream: "missing-upstream"
            weight: 1
"#;

    let result = materialize_yaml(yaml);
    assert!(result.is_err(), "Should reject missing upstream reference");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("upstream") || err.contains("missing"),
        "Error should mention missing upstream, got: {}",
        err
    );
}

#[test]
fn test_retry_max_attempts_zero() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        retry:
          attempts: 0
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let result = compile_yaml(yaml);
    assert!(result.is_err(), "Should reject max_attempts = 0");

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("max_attempts must be >= 1") || err.contains("attempts"),
        "Error should mention max_attempts constraint, got: {}",
        err
    );
}

#[test]
fn test_retry_missing_status_codes() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        retry:
          attempts: 3
          retry_on: ["status_code"]
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let result = compile_yaml(yaml);
    assert!(
        result.is_err(),
        "Should reject retry_on=[status_code] without retryable_status_codes"
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("retryable_status_codes is required") || err.contains("status_codes"),
        "Error should mention missing retryable_status_codes, got: {}",
        err
    );
}

#[test]
fn test_retry_per_try_exceeds_request_timeout() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        timeout: "100ms"
        retry:
          attempts: 3
          retry_on: ["status_code"]
          retryable_status_codes: [503]
          per_try: "200ms"
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let result = compile_yaml(yaml);
    assert!(
        result.is_err(),
        "Should reject per_try_timeout > request_timeout"
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("per_try timeout") && err.contains("exceeds") || err.contains("timeout"),
        "Error should mention timeout hierarchy violation, got: {}",
        err
    );
}

#[test]
fn test_valid_retry_config() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        retry:
          attempts: 3
          retry_on: ["status_code"]
          retryable_status_codes: [503, 504]
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let result = compile_yaml(yaml);
    assert!(
        result.is_ok(),
        "Should accept valid retry configuration: {:?}",
        result.err()
    );
}
