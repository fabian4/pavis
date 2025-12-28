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
