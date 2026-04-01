use anyhow::{Context, Result};
use pavis_codec_api::{Codec, CompactionLevel};
use pavis_codec_serde::{SerdeCodec, SerdeFormat};
use pavis_core::{self as binary};
use pavis_ingest_api::{Artifact, Format, SourceInfo};
use std::path::Path;

pub fn parse_runtime_from_bytes(
    format: SerdeFormat,
    bytes: &[u8],
) -> Result<binary::RuntimeConfig> {
    let ingest_format = match format {
        SerdeFormat::Yaml => Format::Yaml,
        SerdeFormat::Json => Format::Json,
    };
    let env = Artifact::new(bytes.to_vec().into(), ingest_format, SourceInfo::unknown());
    let codec = SerdeCodec { format };
    let validated = codec
        .materialize(env, CompactionLevel::Off)
        .context("Failed to decode config")?;
    Ok(validated.into_inner())
}

pub fn parse_runtime_from_path(path: &Path) -> Result<binary::RuntimeConfig> {
    let bytes = std::fs::read(path).context("Failed to read input file")?;
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("pvs") => pavis_pvs::load(path).context("Failed to load PVS artifact"),
        Some("json") => parse_runtime_from_bytes(SerdeFormat::Json, &bytes),
        Some("yaml") | Some("yml") | None => parse_runtime_from_bytes(SerdeFormat::Yaml, &bytes),
        Some(other) => anyhow::bail!("Unsupported config extension: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_runtime_handles_yaml() {
        let input = b"listeners:\n  - name: default\n    address: 127.0.0.1:8080";
        let config = parse_runtime_from_bytes(SerdeFormat::Yaml, input).expect("yaml");
        assert_eq!(config.listeners[0].address.port(), 8080);
    }

    #[test]
    fn parse_runtime_handles_json() {
        let input = br#"{
            "listeners": [{
                "name": "default",
                "address": "127.0.0.1:9090"
            }]
        }"#;
        let config = parse_runtime_from_bytes(SerdeFormat::Json, input).expect("json");
        assert_eq!(config.listeners[0].address.port(), 9090);
    }

    #[test]
    fn parse_runtime_from_path_handles_pvs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.pvs");
        let listener = binary::ListenerBuilder::new()
            .name(binary::ListenerName("default".to_string()))
            .address("127.0.0.1:8080".parse().expect("addr"))
            .workers(binary::WorkerCount::Auto)
            .tls(binary::TlsConfig::Disabled)
            .build()
            .expect("listener");
        let config = binary::RuntimeConfigBuilder::new()
            .telemetry(binary::Telemetry {
                level: binary::LogLevel::Info,
                pingora: binary::LogLevel::Info,
                service_name: binary::ServiceName("svc".to_string()),
                metrics: binary::Metrics::Disabled,
                access_log: binary::AccessLogPolicy::Stdout,
                tracing: binary::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .expect("config");
        pavis_pvs::write(&path, &config).expect("write");

        let parsed = parse_runtime_from_path(&path).expect("parse pvs");
        assert_eq!(parsed.listeners[0].address.port(), 8080);
    }
}
