use crate::error::{PvsError, PvsResult};
use crate::header::{
    HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader,
    compute_checksum,
};
use pavis_core::RuntimeConfig;
use rkyv::ser::Serializer;
use rkyv::ser::serializers::AllocSerializer;
use std::fs;
use std::path::Path;

pub fn encode(config: &RuntimeConfig) -> PvsResult<Vec<u8>> {
    let mut serializer = AllocSerializer::<1024>::default();
    serializer
        .serialize_value(config)
        .map_err(|e| PvsError::Serialization(format!("{:?}", e)))?;
    let rkyv_bytes = serializer.into_serializer().into_inner();

    let checksum = compute_checksum(&rkyv_bytes);

    let header = PvsHeader {
        magic: *PAVIS_MAGIC,
        version: PAVIS_VERSION,
        algorithm: PAVIS_HASH_ALGORITHM_SHA256,
        checksum,
        _reserved: [0; 20],
    };

    let mut final_bytes = Vec::with_capacity(rkyv_bytes.len() + HEADER_SIZE);
    final_bytes.extend_from_slice(&header.magic);
    final_bytes.extend_from_slice(&header.version.to_le_bytes());
    final_bytes.extend_from_slice(&header.algorithm.to_le_bytes());
    final_bytes.extend_from_slice(&header.checksum);
    final_bytes.extend_from_slice(&header._reserved);
    final_bytes.extend_from_slice(&rkyv_bytes);

    Ok(final_bytes)
}

pub fn write(path: impl AsRef<Path>, config: &RuntimeConfig) -> PvsResult<()> {
    let final_bytes = encode(config)?;
    fs::write(path, final_bytes).map_err(PvsError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write;
    use crate::header::{HEADER_SIZE, PAVIS_MAGIC};
    use pavis_core::{
        AccessLogPolicy, ListenerName, Metrics, RuntimeConfig, RuntimeConfigBuilder, ServiceName,
        Telemetry, TlsConfig, WorkerCount,
    };

    fn minimal_config() -> RuntimeConfig {
        let listener = pavis_core::ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address("127.0.0.1:8080".parse().expect("addr"))
            .workers(WorkerCount::Auto)
            .tls(TlsConfig::Disabled)
            .build()
            .expect("listener");

        RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Info,
                service_name: ServiceName("pavis".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Stdout,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(listener)
            .build()
            .expect("config")
    }

    #[test]
    fn write_emits_header_and_payload() {
        let config = minimal_config();
        let dir = std::env::temp_dir();
        let path = dir.join("pavis_test_config.pvs");
        write(&path, &config).expect("write config");

        let bytes = std::fs::read(&path).expect("read file");
        assert!(bytes.len() > HEADER_SIZE);
        assert_eq!(&bytes[0..4], PAVIS_MAGIC);
        assert!(bytes[44..64].iter().all(|b| *b == 0));
        let _ = std::fs::remove_file(&path);
    }
}
