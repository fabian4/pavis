use pavis_codec_api::{Codec, CodecError, CompactionLevel};
use pavis_codec_serde::{SerdeCodec, SerdeFormat};
use pavis_core::{
    AccessLogPolicy, CoreValidationError, IdleTimeout, Metrics, TlsPolicy, TlsVerify,
};
use pavis_ingest_api::{Artifact, Format, SourceInfo};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn read_fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).expect("read fixture")
}

fn materialize_fixture(format: SerdeFormat, name: &str) -> pavis_core::RuntimeConfig {
    let bytes = read_fixture(name);
    let ingest_format = match format {
        SerdeFormat::Yaml => Format::Yaml,
        SerdeFormat::Json => Format::Json,
    };
    let artifact = Artifact::new(bytes.into(), ingest_format, SourceInfo::unknown());
    let codec = SerdeCodec { format };
    codec
        .materialize(artifact, CompactionLevel::Off)
        .expect("materialize")
        .into_inner()
}

fn compile_fixture(format: SerdeFormat, name: &str) -> pavis_core::RuntimeConfig {
    let bytes = read_fixture(name);
    let ingest_format = match format {
        SerdeFormat::Yaml => Format::Yaml,
        SerdeFormat::Json => Format::Json,
    };
    let artifact = Artifact::new(bytes.into(), ingest_format, SourceInfo::unknown());
    let codec = SerdeCodec { format };
    let checked = codec.check(artifact).expect("check");
    codec.compile(&checked).expect("compile")
}

fn assert_minimal_defaults(cfg: &pavis_core::RuntimeConfig) {
    assert_eq!(cfg.listeners.len(), 1);
    assert_eq!(cfg.listeners[0].address.port(), 8080);
    assert!(cfg.upstreams.is_empty());
    assert!(cfg.routes.is_empty());
    assert!(matches!(cfg.telemetry.metrics, Metrics::Disabled));
    assert!(matches!(cfg.telemetry.access_log, AccessLogPolicy::Stdout));
    assert_eq!(cfg.telemetry.service_name.0, "pavis");
}

#[test]
fn minimal_yaml_and_json_compile_with_defaults() {
    let yaml = materialize_fixture(SerdeFormat::Yaml, "minimal.yaml");
    let json = materialize_fixture(SerdeFormat::Json, "minimal.json");

    assert_minimal_defaults(&yaml);
    assert_minimal_defaults(&json);
}

#[test]
fn full_yaml_and_json_apply_structural_and_semantic_defaults() {
    let yaml = materialize_fixture(SerdeFormat::Yaml, "full.yaml");
    let json = materialize_fixture(SerdeFormat::Json, "full.json");

    for cfg in [&yaml, &json] {
        let upstream = &cfg.upstreams[0];
        assert_eq!(upstream.endpoints[0].weight.0.get(), 1);
        match upstream.pool.idle {
            IdleTimeout::Enabled(d) => assert_eq!(d.0.get(), 60_000),
            IdleTimeout::Disabled => panic!("idle timeout not populated"),
            _ => panic!("unknown idle timeout"),
        }
        match upstream.tls {
            TlsPolicy::Enabled { verify, .. } => {
                assert_eq!(verify, TlsVerify::Full);
            }
            TlsPolicy::Disabled => panic!("tls not enabled"),
            _ => panic!("unknown tls policy"),
        }
        let route = &cfg.routes[0].paths[0];
        assert!(matches!(route.timeout, pavis_core::Timeout::Enabled(_)));
        assert!(matches!(
            route.retry,
            pavis_core::RetryPolicy::Enabled { .. }
        ));
    }
}

#[test]
fn yaml_and_json_defaults_are_consistent() {
    let yaml = materialize_fixture(SerdeFormat::Yaml, "minimal.yaml");
    let json = materialize_fixture(SerdeFormat::Json, "minimal.json");

    assert_eq!(yaml.listeners.len(), json.listeners.len());
    assert_eq!(yaml.listeners[0].address, json.listeners[0].address);
    assert_eq!(yaml.telemetry.service_name.0, json.telemetry.service_name.0);
    assert!(matches!(yaml.telemetry.metrics, Metrics::Disabled));
    assert!(matches!(json.telemetry.metrics, Metrics::Disabled));
    assert!(matches!(yaml.telemetry.access_log, AccessLogPolicy::Stdout));
    assert!(matches!(json.telemetry.access_log, AccessLogPolicy::Stdout));
}

#[test]
fn invalid_yaml_is_rejected() {
    let bytes = read_fixture("invalid.yaml");
    let artifact = Artifact::new(bytes.into(), Format::Yaml, SourceInfo::unknown());
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let err = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect_err("invalid yaml");
    assert!(matches!(err, CodecError::Compile(_)));
}

#[test]
fn invalid_json_is_rejected() {
    let bytes = read_fixture("invalid.json");
    let artifact = Artifact::new(bytes.into(), Format::Json, SourceInfo::unknown());
    let codec = SerdeCodec {
        format: SerdeFormat::Json,
    };
    let err = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect_err("invalid json");
    assert!(matches!(err, CodecError::Compile(_)));
}

#[test]
fn materialize_enforces_core_validation() {
    let yaml = compile_fixture(SerdeFormat::Yaml, "duplicate_upstreams.yaml");
    assert_eq!(yaml.upstreams.len(), 2);

    let json = compile_fixture(SerdeFormat::Json, "duplicate_upstreams.json");
    assert_eq!(json.upstreams.len(), 2);

    for (format, name) in [
        (SerdeFormat::Yaml, "duplicate_upstreams.yaml"),
        (SerdeFormat::Json, "duplicate_upstreams.json"),
    ] {
        let bytes = read_fixture(name);
        let ingest_format = match format {
            SerdeFormat::Yaml => Format::Yaml,
            SerdeFormat::Json => Format::Json,
        };
        let artifact = Artifact::new(bytes.into(), ingest_format, SourceInfo::unknown());
        let codec = SerdeCodec { format };
        let err = codec
            .materialize(artifact, CompactionLevel::Off)
            .expect_err("core validation failure");
        assert!(matches!(
            err,
            CodecError::Core(CoreValidationError::DuplicateUpstream(_))
        ));
    }
}

#[test]

fn materialize_rejects_full_verify_auto_sni_with_ip_endpoint() {
    let yaml = r#"

listeners:

  - name: "default"

    address: "0.0.0.0:8080"

telemetry: {}

upstreams:

  - name: "backend"

    tls:

      enabled: true

      verify_cert: true

      verify_hostname: true

      sni_mode: auto

    endpoints:

      - ip: "127.0.0.1"

        port: 443

routes:

  - host: "*"

    paths:

      - matcher:
          path: !prefix { path: "/" }

        destinations:

          - upstream: "backend"

            weight: 1

"#;

    let artifact = Artifact::new(
        yaml.as_bytes().to_vec().into(),
        Format::Yaml,
        SourceInfo::unknown(),
    );

    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };

    let err = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect_err("expected core validation error");

    match err {
        CodecError::Core(CoreValidationError::UpstreamTlsAutoSniRequiresDns(_)) => {}

        _ => panic!(
            "expected UpstreamTlsAutoSniRequiresDns core error, got {:?}",
            err
        ),
    }
}

// P0 Feature #1: Header/Method Routing Gap - Integration Tests
// These tests verify codec parsing of method and header predicates.

/// Test 1: Codec parses method predicates correctly (GET, POST, etc.)
#[test]
fn codec_parses_method_predicates() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - address: "127.0.0.1"
        port: 8080
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/api" }
          method: "GET"
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let artifact = Artifact::new(
        yaml.as_bytes().to_vec().into(),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let config = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect("materialize");

    let route = &config.into_inner().routes[0].paths[0];
    match &route.matcher.method {
        pavis_core::MethodPredicate::Specific(m) => {
            assert_eq!(m.as_str(), "GET");
        }
        _ => panic!("expected specific method predicate"),
    }
}

/// Test 2: Codec parses single header predicate (exact match)
#[test]
fn codec_parses_single_header_predicate() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - address: "127.0.0.1"
        port: 8080
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/api" }
          headers:
            - name: "x-tenant"
              value: "alice"
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let artifact = Artifact::new(
        yaml.as_bytes().to_vec().into(),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let config = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect("materialize");

    let route = &config.into_inner().routes[0].paths[0];
    match &route.matcher.headers {
        pavis_core::HeaderPredicates::Some(predicates) => {
            assert_eq!(predicates.len(), 1);
            assert_eq!(predicates[0].name.as_str(), "x-tenant");
            match &predicates[0].matcher {
                pavis_core::HeaderMatch::Exact(v) => assert_eq!(v.as_str(), "alice"),
                _ => panic!("expected exact header match"),
            }
        }
        _ => panic!("expected header predicates"),
    }
}

/// Test 3: Codec parses multiple header predicates (AND logic)
#[test]
fn codec_parses_multiple_header_predicates() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - address: "127.0.0.1"
        port: 8080
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/api" }
          headers:
            - name: "x-tenant"
              value: "alice"
            - name: "x-region"
              value: "us-east"
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let artifact = Artifact::new(
        yaml.as_bytes().to_vec().into(),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let config = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect("materialize");

    let route = &config.into_inner().routes[0].paths[0];
    match &route.matcher.headers {
        pavis_core::HeaderPredicates::Some(predicates) => {
            assert_eq!(predicates.len(), 2);
            assert_eq!(predicates[0].name.as_str(), "x-tenant");
            assert_eq!(predicates[1].name.as_str(), "x-region");
        }
        _ => panic!("expected header predicates"),
    }
}

/// Test 4: Codec parses compound matcher (path + method + headers)
#[test]
fn codec_parses_compound_matcher() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - address: "127.0.0.1"
        port: 8080
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/api" }
          method: "POST"
          headers:
            - name: "content-type"
              value: "application/json"
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let artifact = Artifact::new(
        yaml.as_bytes().to_vec().into(),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let config = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect("materialize");

    let route = &config.into_inner().routes[0].paths[0];

    // Verify path
    match &route.matcher.path {
        pavis_core::PathMatch::Prefix { path } => assert_eq!(path.0, "/api"),
        _ => panic!("expected prefix path match"),
    }

    // Verify method
    match &route.matcher.method {
        pavis_core::MethodPredicate::Specific(m) => assert_eq!(m.as_str(), "POST"),
        _ => panic!("expected specific method"),
    }

    // Verify headers
    match &route.matcher.headers {
        pavis_core::HeaderPredicates::Some(predicates) => {
            assert_eq!(predicates.len(), 1);
            assert_eq!(predicates[0].name.as_str(), "content-type");
        }
        _ => panic!("expected header predicates"),
    }
}

/// Test 5: Codec defaults to MethodPredicate::Any when method not specified
#[test]
fn codec_defaults_method_to_any() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - address: "127.0.0.1"
        port: 8080
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/api" }
        destinations:
          - upstream: "backend"
            weight: 1
"#;

    let artifact = Artifact::new(
        yaml.as_bytes().to_vec().into(),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let config = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect("materialize");

    let route = &config.into_inner().routes[0].paths[0];
    assert!(matches!(
        route.matcher.method,
        pavis_core::MethodPredicate::Any
    ));
    assert!(matches!(
        route.matcher.headers,
        pavis_core::HeaderPredicates::None
    ));
}

// P0 Feature #2: Pool Validation - Integration Tests
// These tests verify pool configuration is correctly wired from codec to runtime.

/// Test 6: Codec correctly wires pool.max to ConnectionLimit
#[test]
fn codec_wires_pool_max_to_connection_limit() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    pool:
      max: 256
    endpoints:
      - address: "127.0.0.1"
        port: 8080
"#;

    let artifact = Artifact::new(
        yaml.as_bytes().to_vec().into(),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let config = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect("materialize");

    let upstream = &config.into_inner().upstreams[0];
    assert_eq!(upstream.pool.max.0.get(), 256);
}

/// Test 7: Codec enforces pool.max >= 1 validation at compile time
#[test]
fn codec_rejects_pool_max_zero() {
    let yaml = r#"
listeners:
  - name: "default"
    address: "0.0.0.0:8080"
telemetry: {}
upstreams:
  - name: "backend"
    pool:
      max: 0
    endpoints:
      - address: "127.0.0.1"
        port: 8080
"#;

    let artifact = Artifact::new(
        yaml.as_bytes().to_vec().into(),
        Format::Yaml,
        SourceInfo::unknown(),
    );
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let err = codec
        .materialize(artifact, CompactionLevel::Off)
        .expect_err("pool.max=0 should be rejected");

    // pool.max=0 is rejected at YAML parsing level (NonZeroU32 type constraint)
    assert!(matches!(err, pavis_codec_api::CodecError::Compile(_)));
}
