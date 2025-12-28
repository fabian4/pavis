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

pub fn write(path: impl AsRef<Path>, config: &RuntimeConfig) -> PvsResult<()> {
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

    fs::write(path, final_bytes).map_err(PvsError::Io)?;
    Ok(())
}
