use pavis_core::ValidatedRuntimeConfig;
use pavis_pvs::PvsError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeLoadError {
    #[error(transparent)]
    Pvs(#[from] PvsError),
}

pub type LoadResult<T> = Result<T, RuntimeLoadError>;

/// Orchestrates the loading of a configuration file.
///
/// 1. Reads and deserializes the binary file via `pavis-pvs`.
/// 2. Returns a `RuntimeConfig` trusted to be semantically validated by the producer.
pub fn load_file(path: &str) -> LoadResult<ValidatedRuntimeConfig> {
    if !path.ends_with(".pvs") {
        return Err(RuntimeLoadError::Pvs(PvsError::InvalidExtension(
            PathBuf::from(path),
        )));
    }

    let config = pavis_pvs::load(path)?;
    // SAFETY: `.pvs` artifacts are produced after canonical validation; runtime does not
    // perform semantic inference or mutation after loading. The integrity is guaranteed
    // by the `pavis-pvs` crate which validates checksums and binary layout.
    Ok(unsafe { ValidatedRuntimeConfig::from_trusted(config) })
}

#[cfg(test)]
mod tests {
    use super::load_file;
    use pavis_core::{
        ListenerBuilder, ListenerName, Metrics, RuntimeConfigBuilder, ServiceName, Telemetry,
        WorkerCount,
    };
    use pavis_pvs::PvsError;
    use std::path::PathBuf;

    #[test]
    fn load_file_rejects_non_pvs_extension() {
        let err = load_file("config.yaml").expect_err("expected invalid extension error");
        assert!(matches!(
            err,
            super::RuntimeLoadError::Pvs(PvsError::InvalidExtension(_))
        ));
    }

    fn build_validated_config() -> pavis_core::ValidatedRuntimeConfig {
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address("127.0.0.1:0".parse().unwrap())
            .workers(WorkerCount::Auto)
            .tls(pavis_core::TlsConfig::Disabled)
            .build()
            .expect("listener");

        let config = RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: ServiceName("pavis".to_string()),
                metrics: Metrics::Disabled,
                access_log: pavis_core::AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .expect("config");

        pavis_core::validate_runtime(config).expect("validate")
    }

    fn temp_pvs_path(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        path.push(format!("pavis_{label}_{nonce}.pvs"));
        path
    }

    #[test]
    fn load_file_rejects_version_mismatch() {
        let validated = build_validated_config();
        let mut bytes = pavis_pvs::encode(validated.as_ref()).expect("encode");
        bytes[4..8].copy_from_slice(&(pavis_pvs::PAVIS_VERSION + 1).to_le_bytes());

        let path = temp_pvs_path("version_mismatch");
        std::fs::write(&path, bytes).expect("write pvs");

        let err = load_file(path.to_str().unwrap()).expect_err("version mismatch");
        assert!(matches!(
            err,
            super::RuntimeLoadError::Pvs(PvsError::VersionMismatch { .. })
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_file_rejects_corrupt_pvs() {
        let validated = build_validated_config();
        let mut bytes = pavis_pvs::encode(validated.as_ref()).expect("encode");
        let payload_start = pavis_pvs::HEADER_SIZE;
        bytes[payload_start] ^= 0xFF;

        let path = temp_pvs_path("corrupt");
        std::fs::write(&path, bytes).expect("write pvs");

        let err = load_file(path.to_str().unwrap()).expect_err("corrupt pvs");
        assert!(matches!(
            err,
            super::RuntimeLoadError::Pvs(PvsError::ChecksumMismatch { .. })
                | super::RuntimeLoadError::Pvs(PvsError::CorruptArchive(_))
        ));
        let _ = std::fs::remove_file(path);
    }
}
