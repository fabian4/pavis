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
        }
        match upstream.tls {
            TlsPolicy::Enabled { mode, .. } => {
                assert_eq!(mode, TlsVerify::CertAndHost);
            }
            TlsPolicy::Disabled => panic!("tls not enabled"),
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
