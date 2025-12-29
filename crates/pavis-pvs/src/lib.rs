pub mod error;
pub mod header;
mod read;
mod verify;
mod write;

pub use error::{PvsError, PvsResult};
pub use header::{
    HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsHeader,
    algorithm_label, checksum_hex, compute_checksum,
};

pub use read::read_header;
pub use verify::{PvsHeaderView, VerifiedPvs, inspect, load, read_from_path, verify};
pub use write::write;

#[cfg(test)]
mod tests {
    use super::{PAVIS_MAGIC, compute_checksum};

    #[test]
    fn reexports_are_accessible() {
        assert_eq!(PAVIS_MAGIC, b"PAVS");
        let checksum = compute_checksum(b"payload");
        assert_eq!(checksum.len(), 32);
    }
}
