use anyhow::Result;
use clap::{Parser, Subcommand};
use pavis_codec_serde::SerdeFormat;
use std::path::PathBuf;

mod commands;

use commands::{
    compile_config, convert_to_config, get_default_output, inspect_config, validate_config,
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
    /// Generate a Pavis binary (.pvs) from a high-level config (e.g., YAML/YML/JSON)
    #[command(name = "gen")]
    Generate {
        /// Input config file (YAML/YML/JSON)
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
    /// Check a config file without compiling
    #[command(name = "check")]
    Check {
        /// Input config file (YAML/YML/JSON)
        input: PathBuf,
    },
    /// Convert a Pavis binary file (.pvs) back to YAML/YML/JSON
    #[command(name = "convert")]
    Convert {
        /// Input Pavis file
        input: PathBuf,

        /// Output config file. If omitted, prints to stdout.
        output: Option<PathBuf>,

        /// Output format when writing to stdout or when output extension is missing.
        #[arg(long, default_value = "yaml")]
        format: String,
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
        Commands::Check { input } => validate_config(input),
        Commands::Convert {
            input,
            output,
            format,
        } => {
            let format = parse_format_from_args(output.as_ref(), &format)?;
            convert_to_config(input, output, format)
        }
    }
}

fn parse_format_from_args(output: Option<&PathBuf>, fallback: &str) -> Result<SerdeFormat> {
    let ext = output.and_then(|path| path.extension().and_then(|ext| ext.to_str()));
    match ext {
        Some("json") => Ok(SerdeFormat::Json),
        Some("yaml") | Some("yml") => Ok(SerdeFormat::Yaml),
        Some(other) => anyhow::bail!("Unsupported format: {other}"),
        None => match fallback {
            "json" => Ok(SerdeFormat::Json),
            "yaml" | "yml" => Ok(SerdeFormat::Yaml),
            other => anyhow::bail!("Unsupported format: {other}"),
        },
    }
}
