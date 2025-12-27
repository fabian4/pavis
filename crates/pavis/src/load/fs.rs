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

    if mmap.len() < 8 {
        return Err(anyhow!("Config file too small (must be at least 8 bytes)"));
    }

    let magic = &mmap[0..4];
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());

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

    let payload = &mmap[8..];

    let archived = rkyv::check_archived_root::<RuntimeConfig>(payload)
        .map_err(|e| anyhow!("Binary integrity check failed: {:?}", e))?;

    let wrapper: With<RuntimeConfig, AsOwned> =
        <<RuntimeConfig as Archive>::Archived as rkyv::Deserialize<_, _>>::deserialize(
            archived,
            &mut Infallible,
        )?;

    Ok(wrapper.into_inner())
}
