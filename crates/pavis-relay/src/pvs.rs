use pavis_pvs::{
    HEADER_SIZE, PAVIS_HASH_ALGORITHM_SHA256, PAVIS_MAGIC, PAVIS_VERSION, PvsError, PvsHeader,
    compute_checksum,
};

pub(crate) struct PvsMeta {
    pub(crate) header: PvsHeader,
}

pub(crate) fn validate(bytes: &[u8]) -> Result<PvsMeta, PvsError> {
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

    Ok(PvsMeta { header })
}

pub(crate) fn checksum_hex(header: &PvsHeader) -> String {
    let mut out = String::with_capacity(header.checksum.len() * 2);
    for byte in header.checksum {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

pub(crate) fn algorithm_label(header: &PvsHeader) -> String {
    if header.algorithm == PAVIS_HASH_ALGORITHM_SHA256 {
        "sha256".to_string()
    } else {
        header.algorithm.to_string()
    }
}

fn parse_header(buf: &[u8]) -> PvsHeader {
    let magic = buf[0..4].try_into().expect("magic");
    let version = u32::from_le_bytes(buf[4..8].try_into().expect("version"));
    let algorithm = u32::from_le_bytes(buf[8..12].try_into().expect("algorithm"));
    let checksum = buf[12..44].try_into().expect("checksum");
    let _reserved = buf[44..64].try_into().expect("reserved");

    PvsHeader {
        magic,
        version,
        algorithm,
        checksum,
        _reserved,
    }
}
