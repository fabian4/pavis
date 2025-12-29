use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use pavis_codec_api::Codec;
use pavis_codec_yaml::YamlCodec;
use pavis_pvs as pvs;

pub(crate) fn convert_to_yaml(input_path: PathBuf, output_path: Option<PathBuf>) -> Result<()> {
    let binary_config = pvs::load(&input_path)?;
    let codec = YamlCodec;
    let env = codec
        .decompile(&binary_config)
        .context("Failed to encode YAML")?;

    match output_path {
        Some(path) => {
            fs::write(&path, &env.bytes).context("Failed to write output file")?;
            println!(
                "✅ Successfully converted {:?} to YAML at {:?}",
                input_path, path
            );
        }
        None => {
            let output = std::str::from_utf8(&env.bytes).context("YAML output not UTF-8")?;
            println!("{}", output);
        }
    }
    Ok(())
}
