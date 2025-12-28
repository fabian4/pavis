use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use sha2::{Digest, Sha256};

/// Magic Bytes "PAVS" (Pavilion) to identify valid Pavis Core files.
pub const PAVIS_MAGIC: &[u8; 4] = b"PAVS";

/// Current Protocol Version. Increment this when breaking changes occur.
pub const PAVIS_VERSION: u32 = 0;

/// Serialized header size in bytes.
pub const HEADER_SIZE: usize = 64;

/// Computes the SHA-256 checksum of the given payload.
pub fn compute_checksum(payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hasher.finalize().into()
}

/// Formats an IP address and port into a socket address string.
/// Handles IPv6 addresses by wrapping them in brackets if they don't already have them.
pub fn format_address(ip: &str, port: u16) -> String {
    if ip.contains(':') && !ip.starts_with('[') {
        format!("[{}]:{}", ip, port)
    } else {
        format!("{}:{}", ip, port)
    }
}

/// The Header of a Pavis configuration file.
/// Always present at the beginning of the binary blob.
#[repr(C)]
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy)]
#[archive(check_bytes)]
pub struct PavisHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub algorithm: u32,
    pub checksum: [u8; 32],
    pub _reserved: [u8; 20],
}

impl Default for PavisHeader {
    fn default() -> Self {
        Self {
            magic: *PAVIS_MAGIC,
            version: PAVIS_VERSION,
            algorithm: 0,
            checksum: [0; 32],
            _reserved: [0; 20],
        }
    }
}
