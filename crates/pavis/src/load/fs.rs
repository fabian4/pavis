use anyhow::{Context, Result, anyhow};
use memmap2::Mmap;
use pavis_core::RuntimeConfig;
use rkyv::with::{AsOwned, With};
use rkyv::{Archive, Infallible};
use std::fs::File;

/// Reads a .pvs file, validates headers, and deserializes the content.
pub fn read_pvs_file(path: &str) -> Result<RuntimeConfig> {
    let file =
        File::open(path).with_context(|| format!("Failed to open .pvs config file: {}", path))?;
    // SAFETY: mmap is unsafe because the file could be modified by another process.
    // For a config file loaded at startup, this is generally acceptable risk.
    let mmap = unsafe { Mmap::map(&file).context("Failed to mmap .pvs file")? };

    // Header size is 64 bytes (aligned to 16 bytes)
    const HEADER_SIZE: usize = 64;

    if mmap.len() < HEADER_SIZE {
        return Err(anyhow!("Config file too small (must be at least 64 bytes)"));
    }

    let magic = &mmap[0..4];
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
    let algorithm = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
    let expected_checksum = &mmap[12..44];
    // _reserved is at 44..64

    if magic != pavis_core::PAVIS_MAGIC {
        return Err(anyhow!("Invalid magic bytes in .pvs file. Expected 'PAVS'"));
    }

    if version != pavis_core::PAVIS_VERSION {
        return Err(anyhow!(
            "Version mismatch! File: {}, Proxy: {}. Please recompile config.",
            version,
            pavis_core::PAVIS_VERSION
        ));
    }

    let payload = &mmap[HEADER_SIZE..];

    // Verify Checksum
    if algorithm == 1 {
        let computed_checksum = pavis_core::compute_checksum(payload);
        if computed_checksum != expected_checksum {
            return Err(anyhow!(
                "Checksum mismatch! The configuration file may be corrupted or tampered with."
            ));
        }
    } else if algorithm != 0 {
        return Err(anyhow!("Unsupported hash algorithm: {}", algorithm));
    }

    // Ensure payload is aligned
    // rkyv requires alignment. mmap usually returns page-aligned memory (4096 bytes).
    // HEADER_SIZE is 64, which is divisible by 16.
    // So payload should be aligned to 16 bytes relative to mmap start.
    // Since mmap start is page aligned, payload is 16-byte aligned.
    // However, check_archived_root might be strict.

    let archived = rkyv::check_archived_root::<RuntimeConfig>(payload)
        .map_err(|e| anyhow!("Binary integrity check failed: {:?}", e))?;

    let wrapper: With<RuntimeConfig, AsOwned> =
        <<RuntimeConfig as Archive>::Archived as rkyv::Deserialize<_, _>>::deserialize(
            archived,
            &mut Infallible,
        )?;

    Ok(wrapper.into_inner())
}
