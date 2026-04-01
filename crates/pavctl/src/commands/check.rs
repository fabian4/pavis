use anyhow::{Context, Result};
use std::path::PathBuf;

use pavctl::parse_runtime_from_path;

pub(crate) fn validate_config(input_path: PathBuf, against: Option<PathBuf>) -> Result<()> {
    let runtime = parse_runtime_from_path(&input_path)?;
    println!("✅ Configuration is valid: {:?}", input_path);

    if let Some(against_path) = against {
        let baseline = parse_runtime_from_path(&against_path)?;
        let baseline = pavis_core::validate_runtime(baseline)
            .with_context(|| format!("Failed to validate baseline config: {:?}", against_path))?;
        let runtime = pavis_core::validate_runtime(runtime)
            .with_context(|| format!("Failed to validate candidate config: {:?}", input_path))?;
        pavis_core::ensure_runtime_reload_safe(&baseline, &runtime).with_context(|| {
            format!(
                "Configuration is not runtime reload-safe against {:?}",
                against_path
            )
        })?;
        println!("✅ Runtime reload-safe against: {:?}", against_path);
    }

    Ok(())
}
