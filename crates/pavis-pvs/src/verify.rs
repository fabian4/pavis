use crate::error::{PvsError, PvsResult, ValidatedLoadError};
use crate::header::{
    HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader,
    compute_checksum,
};
use crate::read::parse_header;
use pavis_core::{RuntimeConfig, ValidatedRuntimeConfig, validate_runtime};
use rkyv::Deserialize as _;
use std::fs;
use std::path::Path;

pub fn verify(path: impl AsRef<Path>) -> PvsResult<PvsHeader> {
    let bytes = fs::read(path).map_err(PvsError::Io)?;
    let (header, payload) = verify_bytes(&bytes)?;
    let _archived = rkyv::check_archived_root::<RuntimeConfig>(payload)
        .map_err(|e| PvsError::CorruptArchive(format!("{:?}", e)))?;
    Ok(header)
}

pub fn load(path: impl AsRef<Path>) -> PvsResult<RuntimeConfig> {
    let bytes = fs::read(path).map_err(PvsError::Io)?;
    let (_header, payload) = verify_bytes(&bytes)?;
    let archived = rkyv::check_archived_root::<RuntimeConfig>(payload)
        .map_err(|e| PvsError::CorruptArchive(format!("{:?}", e)))?;
    let config: RuntimeConfig = archived.deserialize(&mut rkyv::Infallible)?;
    Ok(config)
}

pub fn load_validated(
    path: impl AsRef<Path>,
) -> Result<ValidatedRuntimeConfig, ValidatedLoadError> {
    let config = load(path)?;
    Ok(validate_runtime(config)?)
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

#[cfg(test)]
mod tests {
    use super::verify_bytes;
    use crate::error::PvsError;
    use crate::header::{
        HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader,
        compute_checksum,
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
        assert!(matches!(err, PvsError::InvalidMagic));
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

        let err = super::verify(&path).expect_err("corrupt archive");
        assert!(matches!(err, PvsError::CorruptArchive(_)));
        let _ = std::fs::remove_file(&path);
    }
}
