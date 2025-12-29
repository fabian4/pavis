use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use pavctl::parse_runtime_from_bytes;
use pavis_codec_serde::SerdeFormat;

pub(crate) fn validate_config(input_path: PathBuf) -> Result<()> {
    let bytes = fs::read(&input_path).context("Failed to read input file")?;
    let format = match input_path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => SerdeFormat::Json,
        Some("yaml") | Some("yml") | None => SerdeFormat::Yaml,
        Some(other) => anyhow::bail!("Unsupported config extension: {other}"),
    };
    let _runtime = parse_runtime_from_bytes(format, &bytes)?;
    println!("✅ Configuration is valid: {:?}", input_path);
    Ok(())
}
