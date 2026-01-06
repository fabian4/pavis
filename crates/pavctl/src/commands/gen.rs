use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use pavctl::parse_runtime_from_bytes;
use pavis_codec_serde::SerdeFormat;
use pavis_pvs as pvs;

pub(crate) fn compile_config(input_path: PathBuf, output_path: PathBuf) -> Result<()> {
    tracing::info!("Reading config from {:?}", input_path);
    let bytes = fs::read(&input_path).context("Failed to read input file")?;
    let format = match input_path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => SerdeFormat::Json,
        Some("yaml") | Some("yml") | None => SerdeFormat::Yaml,
        Some(other) => anyhow::bail!("Unsupported config extension: {other}"),
    };
    let pavis_config = parse_runtime_from_bytes(format, &bytes)?;

    // Validate local paths for TLS certificates
    for listener in &pavis_config.listeners {
        if let pavis_core::TlsConfig::Enabled {
            cert_path,
            key_path,
            ..
        } = &listener.tls
        {
            if !std::path::Path::new(&cert_path.0).exists() {
                eprintln!(
                    "⚠️  Warning: Certificate file not found locally: {}",
                    cert_path.0
                );
            }
            if !std::path::Path::new(&key_path.0).exists() {
                eprintln!("⚠️  Warning: Key file not found locally: {}", key_path.0);
            }
        }
    }

    // 3. Write to Disk with explicit header
    pvs::write(&output_path, &pavis_config)?;

    let metadata = fs::metadata(&output_path)?;
    println!("✅ Successfully compiled config to {:?}", output_path);
    println!("   Size: {} bytes", metadata.len());
    println!("   Protocol Version: {}", pvs::PAVIS_VERSION);

    Ok(())
}
