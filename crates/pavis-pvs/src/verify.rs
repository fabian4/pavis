use crate::error::{PvsError, PvsResult};
use crate::header::{
    HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader,
    compute_checksum,
};
use crate::read::parse_header;
use pavis_core::RuntimeConfig;
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
