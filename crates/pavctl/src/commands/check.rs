use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use pavctl::parse_yaml_runtime_from_bytes;

pub(crate) fn validate_yaml(input_path: PathBuf) -> Result<()> {
    let bytes = fs::read(&input_path).context("Failed to read input file")?;
    let _runtime = parse_yaml_runtime_from_bytes(&bytes)?;
    println!("✅ Configuration is valid: {:?}", input_path);
    Ok(())
}
