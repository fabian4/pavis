use crate::error::{PvsError, PvsResult};
use crate::header::{
    HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader,
    algorithm_label, checksum_hex, compute_checksum,
};
use crate::read::parse_header;
use pavis_core::RuntimeConfig;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::sync::Arc;

const MAX_PAYLOAD_SIZE: usize = 100 * 1024 * 1024; // 100 MB

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
    bytes: Arc<VerifiedBytes>,
}

#[derive(Debug)]
enum VerifiedBytes {
    Owned(Vec<u8>),
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
        match &*self.bytes {
            VerifiedBytes::Owned(bytes) => bytes,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match Arc::try_unwrap(self.bytes) {
            Ok(inner) => match inner {
                VerifiedBytes::Owned(bytes) => bytes,
            },
            Err(arc) => match &*arc {
                VerifiedBytes::Owned(bytes) => bytes.clone(),
            },
        }
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
    let (header, bytes) = read_verified_file(path)?;
    let payload = &bytes[HEADER_SIZE..];
    let _archived = rkyv::access::<rkyv::Archived<RuntimeConfig>, rancor::Error>(payload)
        .map_err(|e| PvsError::CorruptArchive(format!("{:?}", e)))?;
    Ok(VerifiedPvs {
        header,
        bytes: Arc::new(VerifiedBytes::Owned(bytes)),
    })
}

pub fn verify_file(path: impl AsRef<Path>) -> PvsResult<()> {
    let (_header, bytes) = read_verified_file(path)?;
    let payload = &bytes[HEADER_SIZE..];
    rkyv::access::<rkyv::Archived<RuntimeConfig>, rancor::Error>(payload)
        .map_err(|e| PvsError::CorruptArchive(format!("{:?}", e)))?;
    Ok(())
}

fn verify_bytes(bytes: &[u8]) -> PvsResult<(PvsHeader, &[u8])> {
    if bytes.len() < HEADER_SIZE {
        return Err(PvsError::TooSmall {
            min: HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    if bytes.len() > HEADER_SIZE + MAX_PAYLOAD_SIZE {
        return Err(PvsError::PayloadTooLarge {
            max: MAX_PAYLOAD_SIZE,
            found: bytes.len() - HEADER_SIZE,
        });
    }

    let header = parse_header(&bytes[..HEADER_SIZE])?;

    validate_header(&header)?;

    let payload = &bytes[HEADER_SIZE..];
    let computed_checksum = compute_checksum(payload);
    if computed_checksum != header.checksum {
        return Err(PvsError::ChecksumMismatch {
            expected: to_hex(&header.checksum),
            found: to_hex(&computed_checksum),
        });
    }

    Ok((header, payload))
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn verify_owned(bytes: Vec<u8>) -> PvsResult<VerifiedPvs> {
    let (header, payload) = verify_bytes(&bytes)?;
    let _archived = rkyv::access::<rkyv::Archived<RuntimeConfig>, rancor::Error>(payload)
        .map_err(|e| PvsError::CorruptArchive(format!("{:?}", e)))?;
    Ok(VerifiedPvs {
        header,
        bytes: Arc::new(VerifiedBytes::Owned(bytes)),
    })
}

pub fn load(path: impl AsRef<Path>) -> PvsResult<RuntimeConfig> {
    let (_header, bytes) = read_verified_file(path)?;
    let payload = &bytes[HEADER_SIZE..];
    let archived = rkyv::access::<rkyv::Archived<RuntimeConfig>, rancor::Error>(payload)
        .map_err(|e| PvsError::CorruptArchive(format!("{:?}", e)))?;
    let config: RuntimeConfig = rkyv::deserialize::<RuntimeConfig, rancor::Error>(archived)
        .map_err(|e| PvsError::CorruptArchive(format!("Deserialization error: {:?}", e)))?;
    Ok(config)
}

fn validate_header(header: &PvsHeader) -> PvsResult<()> {
    if &header.magic != PAVIS_MAGIC {
        return Err(PvsError::InvalidMagic {
            expected: String::from_utf8_lossy(PAVIS_MAGIC).to_string(),
            found: String::from_utf8_lossy(&header.magic).to_string(),
        });
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

    Ok(())
}

fn read_verified_file(path: impl AsRef<Path>) -> PvsResult<(PvsHeader, Vec<u8>)> {
    let mut file = fs::File::open(path).map_err(PvsError::Io)?;
    let metadata = file.metadata().map_err(PvsError::Io)?;
    let file_len = metadata.len().min(usize::MAX as u64) as usize;

    if file_len < HEADER_SIZE {
        return Err(PvsError::TooSmall {
            min: HEADER_SIZE,
            actual: file_len,
        });
    }

    if file_len > HEADER_SIZE + MAX_PAYLOAD_SIZE {
        return Err(PvsError::PayloadTooLarge {
            max: MAX_PAYLOAD_SIZE,
            found: file_len - HEADER_SIZE,
        });
    }

    let mut header_buf = [0u8; HEADER_SIZE];
    if let Err(err) = file.read_exact(&mut header_buf) {
        if err.kind() == ErrorKind::UnexpectedEof {
            let actual = file
                .metadata()
                .map_err(PvsError::Io)?
                .len()
                .min(usize::MAX as u64) as usize;
            return Err(PvsError::TooSmall {
                min: HEADER_SIZE,
                actual,
            });
        }
        return Err(PvsError::Io(err));
    }

    let header = parse_header(&header_buf)?;
    validate_header(&header)?;

    let mut bytes = Vec::with_capacity(file_len);
    bytes.extend_from_slice(&header_buf);

    let mut hasher = Sha256::new();
    let mut payload_len = 0usize;
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).map_err(PvsError::Io)?;
        if n == 0 {
            break;
        }
        payload_len = payload_len.saturating_add(n);
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(PvsError::PayloadTooLarge {
                max: MAX_PAYLOAD_SIZE,
                found: payload_len,
            });
        }
        hasher.update(&buf[..n]);
        bytes.extend_from_slice(&buf[..n]);
    }

    let computed_checksum: [u8; 32] = hasher.finalize().into();
    if computed_checksum != header.checksum {
        return Err(PvsError::ChecksumMismatch {
            expected: to_hex(&header.checksum),
            found: to_hex(&computed_checksum),
        });
    }

    Ok((header, bytes))
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
    use pavis_core::{
        AccessLogPolicy, ListenerName, Metrics, RuntimeConfig, RuntimeConfigBuilder, ServiceName,
        Telemetry, TlsConfig, WorkerCount,
    };

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
        assert!(matches!(err, PvsError::InvalidMagic { .. }));
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
        assert!(matches!(err, PvsError::ChecksumMismatch { .. }));
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

    #[test]
    fn verify_bytes_rejects_large_payload() {
        let payload = vec![0u8; super::MAX_PAYLOAD_SIZE + 1];
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes.extend_from_slice(&payload);

        // We don't even need valid header because size check happens first (after min check)
        // But to be clean let's make it look like a PVS
        let header = PvsHeader::default();
        bytes[0..4].copy_from_slice(&header.magic);

        let err = verify_bytes(&bytes).expect_err("too large");
        assert!(matches!(err, PvsError::PayloadTooLarge { .. }));
    }

    #[test]
    fn verify_bytes_at_max_payload_limit() {
        let payload = vec![0u8; super::MAX_PAYLOAD_SIZE];
        let checksum = compute_checksum(&payload);
        let header = PvsHeader {
            checksum,
            ..PvsHeader::default()
        };

        let mut bytes = Vec::with_capacity(HEADER_SIZE + payload.len());
        bytes.extend_from_slice(&header.magic);
        bytes.extend_from_slice(&header.version.to_le_bytes());
        bytes.extend_from_slice(&header.algorithm.to_le_bytes());
        bytes.extend_from_slice(&header.checksum);
        bytes.extend_from_slice(&header._reserved);
        bytes.extend_from_slice(&payload);

        let (parsed, _) = verify_bytes(&bytes).expect("at max limit should pass");
        assert_eq!(parsed.checksum, checksum);
    }

    #[test]
    fn verify_bytes_error_context_magic() {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"BAD!");
        let err = verify_bytes(&bytes).expect_err("invalid magic");
        if let PvsError::InvalidMagic { expected, found } = err {
            assert_eq!(expected, "PAVS");
            assert_eq!(found, "BAD!");
        } else {
            panic!("Expected InvalidMagic, got {:?}", err);
        }
    }

    #[test]
    fn verify_bytes_error_context_checksum() {
        let payload = b"hello";
        let header = PvsHeader {
            checksum: [0xaa; 32],
            ..PvsHeader::default()
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&header.magic);
        bytes.extend_from_slice(&header.version.to_le_bytes());
        bytes.extend_from_slice(&header.algorithm.to_le_bytes());
        bytes.extend_from_slice(&header.checksum);
        bytes.extend_from_slice(&header._reserved);
        bytes.extend_from_slice(payload);

        let err = verify_bytes(&bytes).expect_err("checksum mismatch");
        if let PvsError::ChecksumMismatch { expected, found } = err {
            assert_eq!(
                expected,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            );
            // SHA256 of "hello" starts with 2cf24dba5...
            assert!(
                found.starts_with(
                    "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                )
            );
        } else {
            panic!("Expected ChecksumMismatch, got {:?}", err);
        }
    }

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

    #[test]
    fn verify_file_success() {
        let config = minimal_config();
        let dir = std::env::temp_dir();
        let path = dir.join("pavis_verify_file.pvs");
        write(&path, &config).expect("write config");

        assert!(super::verify_file(&path).is_ok());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn into_bytes_mapped() {
        let config = minimal_config();
        let dir = std::env::temp_dir();
        let path = dir.join("pavis_into_bytes_mapped.pvs");
        write(&path, &config).expect("write config");

        let verified = super::read_from_path(&path).expect("read from path");
        let bytes = verified.into_bytes();
        assert_eq!(
            bytes.len(),
            std::fs::metadata(&path).unwrap().len() as usize
        );

        let _ = std::fs::remove_file(&path);
    }
}
