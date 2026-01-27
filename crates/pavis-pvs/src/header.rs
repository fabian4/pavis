use sha2::{Digest, Sha256};

/// Magic Bytes "PAVS" (Pavilion) to identify valid PVS files.
pub const PAVIS_MAGIC: &[u8; 4] = b"PAVS";

/// Current PVS ABI version supported by this runtime build.
pub const PAVIS_VERSION: u32 = 0;

/// Hash algorithm ID for SHA-256 (ABI-frozen for `format_version = 0`).
pub const PAVIS_HASH_ALGORITHM_SHA256: u32 = 1;

/// Serialized header size in bytes.
pub const HEADER_SIZE: usize = 64;

/// Computes the SHA-256 checksum of the given payload.
pub fn compute_checksum(payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hasher.finalize().into()
}

/// Formats a header checksum as lowercase hex.
pub fn checksum_hex(header: &PvsHeader) -> String {
    let mut out = String::with_capacity(header.checksum.len() * 2);
    for byte in header.checksum {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Returns a human-readable algorithm label.
pub fn algorithm_label(header: &PvsHeader) -> String {
    if header.algorithm == PAVIS_HASH_ALGORITHM_SHA256 {
        "sha256".to_string()
    } else {
        header.algorithm.to_string()
    }
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

#[cfg(test)]
mod tests {
    use super::{
        PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader, algorithm_label,
        compute_checksum,
    };

    #[test]
    fn checksum_is_deterministic() {
        let payload = b"payload";
        assert_eq!(compute_checksum(payload), compute_checksum(payload));
    }

    #[test]
    fn default_header_uses_constants() {
        let header = PvsHeader::default();
        assert_eq!(header.magic, *PAVIS_MAGIC);
        assert_eq!(header.version, PAVIS_VERSION);
        assert_eq!(header.algorithm, PAVIS_HASH_ALGORITHM_SHA256);
    }

    #[test]
    fn algorithm_label_returns_numeric_for_unknown() {
        let header = PvsHeader {
            algorithm: 7,
            ..PvsHeader::default()
        };
        assert_eq!(algorithm_label(&header), "7");
    }
}
