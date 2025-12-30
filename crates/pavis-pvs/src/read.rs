use crate::error::{PvsError, PvsResult};
use crate::header::{HEADER_SIZE, PvsHeader};
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

pub fn read_header(path: impl AsRef<Path>) -> PvsResult<PvsHeader> {
    let mut file = File::open(path).map_err(PvsError::Io)?;
    let mut buf = [0u8; HEADER_SIZE];
    if let Err(err) = file.read_exact(&mut buf) {
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
    Ok(parse_header(&buf))
}

pub(crate) fn parse_header(buf: &[u8]) -> PvsHeader {
    let magic = buf[0..4].try_into().unwrap();
    let version = u32::from_le_bytes(buf[4..8].try_into().unwrap());
    let algorithm = u32::from_le_bytes(buf[8..12].try_into().unwrap());
    let checksum = buf[12..44].try_into().unwrap();
    let _reserved = buf[44..64].try_into().unwrap();

    PvsHeader {
        magic,
        version,
        algorithm,
        checksum,
        _reserved,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_header, read_header};
    use crate::error::PvsError;
    use crate::header::{PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION};

    #[test]
    fn parse_header_reads_fields() {
        let mut buf = [0u8; 64];
        buf[0..4].copy_from_slice(PAVIS_MAGIC);
        buf[4..8].copy_from_slice(&PAVIS_VERSION.to_le_bytes());
        buf[8..12].copy_from_slice(&PAVIS_HASH_ALGORITHM_SHA256.to_le_bytes());
        buf[12..44].copy_from_slice(&[1u8; 32]);
        let header = parse_header(&buf);
        assert_eq!(header.magic, *PAVIS_MAGIC);
        assert_eq!(header.version, PAVIS_VERSION);
        assert_eq!(header.algorithm, PAVIS_HASH_ALGORITHM_SHA256);
        assert_eq!(header.checksum, [1u8; 32]);
    }

    #[test]
    fn read_header_rejects_too_small_file() {
        let path = std::env::temp_dir().join(format!(
            "pavis_read_header_small_{}.pvs",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::write(&path, [0u8; 4]).expect("write small file");

        let err = read_header(&path).expect_err("too small");
        assert!(matches!(err, PvsError::TooSmall { .. }));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn read_header_propagates_io_error() {
        let dir = std::env::temp_dir().join(format!(
            "pavis_read_header_dir_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create dir");

        let err = read_header(&dir).expect_err("io error");
        assert!(matches!(err, PvsError::Io(_)));

        let _ = std::fs::remove_dir_all(dir);
    }
}
