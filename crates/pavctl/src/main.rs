use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;

use commands::{
    compile_config, convert_to_yaml, get_default_output, inspect_config, validate_yaml,
};

#[derive(Parser)]
#[command(name = "pavctl")]
#[command(version, about = "The developer tool for Pavis", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a Pavis binary (.pvs) from a high-level config (e.g., YAML)
    #[command(name = "gen")]
    Generate {
        /// Input config file (YAML)
        input: PathBuf,

        /// Output Pavis file (.pvs). Defaults to input name with .pvs extension.
        output: Option<PathBuf>,
    },
    /// View a Pavis binary file (.pvs)
    #[command(name = "view")]
    View {
        /// Show hex dump of the payload
        #[arg(short = 'x', long)]
        hex: bool,

        /// Input Pavis file
        input: PathBuf,
    },
    /// Check a YAML config file without compiling
    #[command(name = "check")]
    Check {
        /// Input YAML file
        input: PathBuf,
    },
    /// Convert a Pavis binary file (.pvs) back to YAML
    #[command(name = "convert")]
    Convert {
        /// Input Pavis file
        input: PathBuf,

        /// Output YAML file. Defaults to input name with .yaml extension.
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { input, output } => {
            let out = output.unwrap_or_else(|| get_default_output(&input, "pvs"));
            compile_config(input, out)
        }
        Commands::View { input, hex } => inspect_config(input, hex),
        Commands::Check { input } => validate_yaml(input),
        Commands::Convert { input, output } => {
            let out = output.unwrap_or_else(|| get_default_output(&input, "yaml"));
            convert_to_yaml(input, Some(out))
        }
    }
}
