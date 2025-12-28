use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

use pavis_adapter_yaml::config as yaml;
use pavis_core::{self as binary, Config, ConfigSource};
use pavis_pvs as pvs;

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
    let yaml_config = yaml::YamlConfig::load(ConfigSource::File(&input_path))
        .context("Failed to load YAML config")?;
    Config::validate(&yaml_config).context("Invalid configuration")?;

    // Convert to Pavis Structs
    let pavis_config: binary::RuntimeConfig = yaml_config.build()?;

    // 3. Write to Disk with explicit header
    pvs::write(&output_path, &pavis_config)?;
    tracing::info!(
        "Successfully compiled config to {:?} (Size: {} bytes)",
        output_path,
        fs::metadata(&output_path)?.len()
    );

    Ok(())
}
