use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rkyv::ser::{Serializer, serializers::AllocSerializer};
use std::fs;
use std::path::PathBuf;

use pavis_adapter_yaml::config as yaml;
use pavis_core as binary; // User Input (YAML) // Binary Protocol (Rkyv)

#[derive(Parser)]
#[command(name = "pavis-xds")]
#[command(about = "The Control Plane & Compiler for Pavis", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a YAML config file into a Pavis binary
    Compile {
        /// Input YAML file
        #[arg(short, long)]
        input: PathBuf,

        /// Output Pavis file
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile { input, output } => compile_config(input, output),
    }
}

fn compile_config(input_path: PathBuf, output_path: PathBuf) -> Result<()> {
    tracing::info!("Reading YAML config from {:?}", input_path);
    let content = fs::read_to_string(&input_path).context("Failed to read input file")?;

    // 1. Deserialize YAML
    let yaml_config: yaml::YamlConfig =
        serde_yaml::from_str(&content).context("Failed to parse YAML")?;

    // Validate the config (shared logic)
    // Note: We create a ValidateConfig wrapper but we just need the inner or check result.
    // We ignore the returned ValidatedConfig wrapper and just use the validated data for conversion,
    // or we could use the validated struct if conversion logic expected it.
    // For now, we just validate to ensure correctness.
    yaml_config
        .clone()
        .validate()
        .context("Invalid configuration")?;

    // 2. Convert to Pavis Structs
    let pavis_config: binary::RuntimeConfig = yaml_config.try_into()?;

    // 3. Serialize to Bytes
    let mut serializer = AllocSerializer::<1024>::default();
    serializer
        .serialize_value(&pavis_config)
        .context("Failed to serialize to Pavis")?;
    let rkyv_bytes = serializer.into_serializer().into_inner();

    // 4. Compute Checksum
    let checksum = binary::compute_checksum(&rkyv_bytes);

    // 5. Write to Disk with explicit header
    let header = binary::PavisHeader {
        magic: *binary::PAVIS_MAGIC,
        version: binary::PAVIS_VERSION,
        algorithm: 1, // SHA-256
        checksum,
        _reserved: [0; 20],
    };

    let mut final_bytes = Vec::with_capacity(rkyv_bytes.len() + binary::HEADER_SIZE);
    final_bytes.extend_from_slice(&header.magic);
    final_bytes.extend_from_slice(&header.version.to_le_bytes());
    final_bytes.extend_from_slice(&header.algorithm.to_le_bytes());
    final_bytes.extend_from_slice(&header.checksum);
    final_bytes.extend_from_slice(&header._reserved);
    final_bytes.extend_from_slice(&rkyv_bytes);

    fs::write(&output_path, final_bytes).context("Failed to write output file")?;
    tracing::info!(
        "Successfully compiled config to {:?} (Size: {} bytes)",
        output_path,
        fs::metadata(&output_path)?.len()
    );

    Ok(())
}
