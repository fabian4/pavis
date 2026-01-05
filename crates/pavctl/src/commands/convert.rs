use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use pavis_codec_serde::SerdeFormat;
use pavis_codec_serde::config::SerdeConfig;
use pavis_codec_serde::serde_helpers::emit_with_format;
use pavis_pvs as pvs;

pub(crate) fn convert_to_config(
    input_path: PathBuf,
    output_path: Option<PathBuf>,
    format: SerdeFormat,
) -> Result<()> {
    let binary_config = pvs::load(&input_path)?;
    let format = match output_path
        .as_ref()
        .and_then(|path| path.extension().and_then(|ext| ext.to_str()))
    {
        Some("json") => SerdeFormat::Json,
        Some("yaml") | Some("yml") => SerdeFormat::Yaml,
        Some(other) => anyhow::bail!("Unsupported output extension: {other}"),
        None => format,
    };
    let config: SerdeConfig = binary_config.into();
    let bytes = emit_with_format(format, &config).context("Failed to encode config")?;

    match output_path {
        Some(path) => {
            fs::write(&path, &bytes).context("Failed to write output file")?;
            println!("✅ Successfully converted {:?} to {:?}", input_path, path);
        }
        None => {
            let output = std::str::from_utf8(&bytes).context("Output not UTF-8")?;
            println!("{}", output);
        }
    }
    Ok(())
}
