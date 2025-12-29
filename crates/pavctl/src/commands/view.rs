use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use pavctl::{format_config, format_header, format_stats};
use pavis_pvs as pvs;

pub(crate) fn inspect_config(input_path: PathBuf, hex: bool) -> Result<()> {
    let header = pvs::read_header(&input_path)?;
    print!("{}", format_header(&header));

    let total_size = fs::metadata(&input_path)
        .context("Failed to read input file metadata")?
        .len();
    let config = pvs::load(&input_path)?;
    print!("{}", format_config(&config));
    print!("{}", format_stats(&config, total_size));

    if hex {
        let bytes = fs::read(&input_path).context("Failed to read input file")?;
        println!("--- Payload Hex Dump ---");
        let payload = &bytes[pvs::HEADER_SIZE..];
        println!("{}", hex::encode(payload));
    }

    Ok(())
}
