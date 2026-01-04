use crate::error::{PvsError, PvsResult};
use crate::header::{
    HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader,
    algorithm_label, checksum_hex, compute_checksum,
};
use crate::read::parse_header;
use pavis_core::RuntimeConfig;
use rkyv::Deserialize as _;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PvsHeaderView {
    header: PvsHeader,
}

impl PvsHeaderView {
    pub fn header(&self) -> &PvsHeader {
        &self.header
    }

    pub fn version(&self) -> u32 {
        self.header.version
    }

    pub fn algorithm(&self) -> u32 {
        self.header.algorithm
    }

    pub fn checksum(&self) -> [u8; 32] {
        self.header.checksum
    }

    pub fn checksum_hex(&self) -> String {
        checksum_hex(&self.header)
    }

    pub fn algorithm_label(&self) -> String {
        algorithm_label(&self.header)
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedPvs {
    header: PvsHeader,
    bytes: Vec<u8>,
}

impl VerifiedPvs {
    pub fn header(&self) -> &PvsHeader {
        &self.header
    }

    pub fn version(&self) -> u32 {
        self.header.version
    }

    pub fn algorithm(&self) -> u32 {
        self.header.algorithm
    }

    pub fn checksum(&self) -> [u8; 32] {
        self.header.checksum
    }

    pub fn checksum_hex(&self) -> String {
        checksum_hex(&self.header)
    }

    pub fn algorithm_label(&self) -> String {
        algorithm_label(&self.header)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub fn inspect(bytes: &[u8]) -> PvsResult<PvsHeaderView> {
    let (header, _payload) = verify_bytes(bytes)?;
    Ok(PvsHeaderView { header })
}

pub fn verify(bytes: &[u8]) -> PvsResult<VerifiedPvs> {
    verify_owned(bytes.to_vec())
}

pub fn read_from_path(path: impl AsRef<Path>) -> PvsResult<VerifiedPvs> {
    let bytes = fs::read(path).map_err(PvsError::Io)?;
    verify_owned(bytes)
}

fn verify_bytes(bytes: &[u8]) -> PvsResult<(PvsHeader, &[u8])> {
    if bytes.len() < HEADER_SIZE {
        return Err(PvsError::TooSmall {
            min: HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    let header = parse_header(&bytes[..HEADER_SIZE]);

    if &header.magic != PAVIS_MAGIC {
        return Err(PvsError::InvalidMagic);
    }

    if header.version != PAVIS_VERSION {
        return Err(PvsError::VersionMismatch {
            file: header.version,
            expected: PAVIS_VERSION,
        });
    }

    if header.algorithm != PAVIS_HASH_ALGORITHM_SHA256 {
        return Err(PvsError::UnsupportedAlgorithm(header.algorithm));
    }

    let payload = &bytes[HEADER_SIZE..];
    let computed_checksum = compute_checksum(payload);
    if computed_checksum != header.checksum {
        return Err(PvsError::ChecksumMismatch);
    }

    Ok((header, payload))
}

fn verify_owned(bytes: Vec<u8>) -> PvsResult<VerifiedPvs> {
    let (header, payload) = verify_bytes(&bytes)?;
    let _archived = rkyv::check_archived_root::<RuntimeConfig>(payload)
        .map_err(|e| PvsError::CorruptArchive(format!("{:?}", e)))?;
    Ok(VerifiedPvs { header, bytes })
}

pub fn load(path: impl AsRef<Path>) -> PvsResult<RuntimeConfig> {
    let bytes = fs::read(path).map_err(PvsError::Io)?;
    let (_header, payload) = verify_bytes(&bytes)?;
    let archived = rkyv::check_archived_root::<RuntimeConfig>(payload)
        .map_err(|e| PvsError::CorruptArchive(format!("{:?}", e)))?;
    let config: RuntimeConfig = archived.deserialize(&mut rkyv::Infallible)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::{inspect, verify, verify_bytes};
    use crate::error::PvsError;
    use crate::header::{
        HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader,
        compute_checksum,
    };
    use crate::write::write;
    use pavis_core::{Listener, RuntimeConfig, TelemetryConfig};

    #[test]
    fn verify_bytes_rejects_short_payload() {
        let err = verify_bytes(&[]).expect_err("too small");
        assert!(matches!(err, PvsError::TooSmall { .. }));
    }

    #[test]
    fn verify_bytes_accepts_valid_header() {
        let payload = b"payload";
        let checksum = compute_checksum(payload);
        let header = PvsHeader {
            magic: *PAVIS_MAGIC,
            version: PAVIS_VERSION,
            algorithm: PAVIS_HASH_ALGORITHM_SHA256,
            checksum,
            _reserved: [1; 20],
        };

        let mut bytes = Vec::with_capacity(HEADER_SIZE + payload.len());
        bytes.extend_from_slice(&header.magic);
        bytes.extend_from_slice(&header.version.to_le_bytes());
        bytes.extend_from_slice(&header.algorithm.to_le_bytes());
        bytes.extend_from_slice(&header.checksum);
        bytes.extend_from_slice(&header._reserved);
        bytes.extend_from_slice(payload);

        let (parsed, parsed_payload) = verify_bytes(&bytes).expect("valid bytes");
        assert_eq!(parsed, header);
        assert_eq!(parsed_payload, payload);
    }

    #[test]
    fn verify_bytes_rejects_invalid_magic() {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NOPE");
        let err = verify_bytes(&bytes).expect_err("invalid magic");
        assert!(matches!(err, PvsError::InvalidMagic));
    }

    #[test]
    fn verify_bytes_rejects_version_mismatch() {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(PAVIS_MAGIC);
        bytes[4..8].copy_from_slice(&(PAVIS_VERSION + 1).to_le_bytes());
        bytes[8..12].copy_from_slice(&PAVIS_HASH_ALGORITHM_SHA256.to_le_bytes());
        let err = verify_bytes(&bytes).expect_err("version mismatch");
        assert!(matches!(err, PvsError::VersionMismatch { .. }));
    }

    #[test]
    fn verify_bytes_rejects_unsupported_algorithm() {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(PAVIS_MAGIC);
        bytes[4..8].copy_from_slice(&PAVIS_VERSION.to_le_bytes());
        bytes[8..12].copy_from_slice(&(PAVIS_HASH_ALGORITHM_SHA256 + 1).to_le_bytes());
        let err = verify_bytes(&bytes).expect_err("unsupported algorithm");
        assert!(matches!(err, PvsError::UnsupportedAlgorithm(_)));
    }

    #[test]
    fn verify_bytes_rejects_truncated_payload() {
        let payload = b"payload";
        let checksum = compute_checksum(payload);
        let header = PvsHeader {
            magic: *PAVIS_MAGIC,
            version: PAVIS_VERSION,
            algorithm: PAVIS_HASH_ALGORITHM_SHA256,
            checksum,
            _reserved: [0; 20],
        };

        let mut bytes = Vec::with_capacity(HEADER_SIZE + payload.len());
        bytes.extend_from_slice(&header.magic);
        bytes.extend_from_slice(&header.version.to_le_bytes());
        bytes.extend_from_slice(&header.algorithm.to_le_bytes());
        bytes.extend_from_slice(&header.checksum);
        bytes.extend_from_slice(&header._reserved);
        bytes.extend_from_slice(&payload[..3]);

        let err = verify_bytes(&bytes).expect_err("checksum mismatch");
        assert!(matches!(err, PvsError::ChecksumMismatch));
    }

    #[test]
    fn verify_rejects_truncated_archive_payload() {
        let payload = [1u8; 5];
        let checksum = compute_checksum(&payload);
        let header = PvsHeader {
            magic: *PAVIS_MAGIC,
            version: PAVIS_VERSION,
            algorithm: PAVIS_HASH_ALGORITHM_SHA256,
            checksum,
            _reserved: [0; 20],
        };

        let mut bytes = Vec::with_capacity(HEADER_SIZE + payload.len());
        bytes.extend_from_slice(&header.magic);
        bytes.extend_from_slice(&header.version.to_le_bytes());
        bytes.extend_from_slice(&header.algorithm.to_le_bytes());
        bytes.extend_from_slice(&header.checksum);
        bytes.extend_from_slice(&header._reserved);
        bytes.extend_from_slice(&payload);

        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "pavis_truncated_archive_{}.pvs",
            std::process::id()
        ));
        std::fs::write(&path, &bytes).expect("write truncated payload");

        let err = super::read_from_path(&path).expect_err("corrupt archive");
        assert!(matches!(err, PvsError::CorruptArchive(_)));
        let _ = std::fs::remove_file(&path);
    }

    fn minimal_config() -> RuntimeConfig {
        RuntimeConfig {
            listeners: vec![Listener {
                name: "default".to_string(),
                listen_addr: "127.0.0.1:8080".parse().expect("addr"),
                worker_threads: None,
                tls: None,
            }],
            telemetry: TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: Default::default(),
                tracing: None,
            },
            upstreams: Vec::new(),
            routes: Vec::new(),
        }
    }

    #[test]
    fn inspect_reports_header_fields() {
        let config = minimal_config();
        let dir = std::env::temp_dir();
        let path = dir.join("pavis_inspect_config.pvs");
        write(&path, &config).expect("write config");

        let bytes = std::fs::read(&path).expect("read file");
        let view = inspect(&bytes).expect("inspect");
        assert_eq!(view.version(), PAVIS_VERSION);
        assert_eq!(view.algorithm(), PAVIS_HASH_ALGORITHM_SHA256);
        assert_eq!(view.header().magic, *PAVIS_MAGIC);
        assert!(!view.checksum_hex().is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn verify_returns_bytes_and_header() {
        let config = minimal_config();
        let dir = std::env::temp_dir();
        let path = dir.join("pavis_verify_config.pvs");
        write(&path, &config).expect("write config");

        let bytes = std::fs::read(&path).expect("read file");
        let verified = verify(&bytes).expect("verify");
        assert_eq!(verified.bytes(), bytes.as_slice());
        assert_eq!(verified.header().version, PAVIS_VERSION);
        assert_eq!(verified.algorithm(), PAVIS_HASH_ALGORITHM_SHA256);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn inspect_and_verify_expose_checksum_helpers() {
        let config = minimal_config();
        let dir = std::env::temp_dir();
        let path = dir.join("pavis_verify_helpers.pvs");
        write(&path, &config).expect("write config");

        let bytes = std::fs::read(&path).expect("read file");
        let view = inspect(&bytes).expect("inspect");
        assert_eq!(view.checksum(), view.header().checksum);
        assert_eq!(view.algorithm_label(), "sha256");

        let verified = verify(&bytes).expect("verify");
        assert_eq!(verified.checksum(), verified.header().checksum);
        assert_eq!(verified.algorithm_label(), "sha256");
        assert_eq!(verified.into_bytes(), bytes);

        let _ = std::fs::remove_file(&path);
    }
}
