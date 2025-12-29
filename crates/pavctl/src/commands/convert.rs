use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use pavis_codec_api::Codec;
use pavis_codec_serde::{SerdeCodec, SerdeFormat};
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
    let codec = SerdeCodec { format };
    let env = codec
        .decompile(&binary_config)
        .context("Failed to encode config")?;

    match output_path {
        Some(path) => {
            fs::write(&path, &env.bytes).context("Failed to write output file")?;
            println!("✅ Successfully converted {:?} to {:?}", input_path, path);
        }
        None => {
            let output = std::str::from_utf8(&env.bytes).context("Output not UTF-8")?;
            println!("{}", output);
        }
    }
    Ok(())
}
