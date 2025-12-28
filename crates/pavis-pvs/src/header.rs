use sha2::{Digest, Sha256};

/// Magic Bytes "PAVS" (Pavilion) to identify valid PVS files.
pub const PAVIS_MAGIC: &[u8; 4] = b"PAVS";

/// Current protocol version. Increment this when breaking changes occur.
pub const PAVIS_VERSION: u32 = 0;

/// Hash algorithm ID for SHA-256.
pub const PAVIS_HASH_ALGORITHM_SHA256: u32 = 1;

/// Serialized header size in bytes.
pub const HEADER_SIZE: usize = 64;

/// Computes the SHA-256 checksum of the given payload.
pub fn compute_checksum(payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hasher.finalize().into()
}

/// The header of a PVS configuration file.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PvsHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub algorithm: u32,
    pub checksum: [u8; 32],
    pub _reserved: [u8; 20],
}

impl Default for PvsHeader {
    fn default() -> Self {
        Self {
            magic: *PAVIS_MAGIC,
            version: PAVIS_VERSION,
            algorithm: PAVIS_HASH_ALGORITHM_SHA256,
            checksum: [0; 32],
            _reserved: [0; 20],
        }
    }
}
