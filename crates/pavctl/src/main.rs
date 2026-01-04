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
    run(cli)
}

fn run(cli: Cli) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::{Cli, Commands, parse_format_from_args, run};
    use crate::commands::compile_config;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(prefix: &str, ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{nanos}.{ext}"))
    }

    fn write_yaml(path: &PathBuf) {
        let content = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - address: "127.0.0.1"
        port: 8081
routes: []
"#;
        fs::write(path, content).expect("write yaml");
    }

    #[test]
    fn run_dispatches_check_and_view() {
        let yaml_path = unique_path("pavctl_main", "yaml");
        let pvs_path = unique_path("pavctl_main", "pvs");
        write_yaml(&yaml_path);
        compile_config(yaml_path.clone(), pvs_path.clone()).expect("compile");

        run(Cli {
            command: Commands::Check {
                input: yaml_path.clone(),
            },
        })
        .expect("check");
        run(Cli {
            command: Commands::View {
                input: pvs_path.clone(),
                hex: false,
            },
        })
        .expect("view");

        let _ = fs::remove_file(&yaml_path);
        let _ = fs::remove_file(&pvs_path);
    }

    #[test]
    fn run_dispatches_gen_and_convert() {
        let yaml_path = unique_path("pavctl_gen", "yaml");
        let pvs_path = unique_path("pavctl_gen", "pvs");
        let conv_path = unique_path("pavctl_gen_conv", "yaml");
        write_yaml(&yaml_path);

        // Test Generate
        run(Cli {
            command: Commands::Generate {
                input: yaml_path.clone(),
                output: Some(pvs_path.clone()),
            },
        })
        .expect("generate");
        assert!(pvs_path.exists());

        // Test Convert
        run(Cli {
            command: Commands::Convert {
                input: pvs_path.clone(),
                output: Some(conv_path.clone()),
                format: "yaml".to_string(),
            },
        })
        .expect("convert");
        assert!(conv_path.exists());

        let _ = fs::remove_file(&yaml_path);
        let _ = fs::remove_file(&pvs_path);
        let _ = fs::remove_file(&conv_path);
    }

    #[test]
    fn parse_format_from_args_rejects_unknowns() {
        let output = PathBuf::from("config.toml");
        let err = parse_format_from_args(Some(&output), "yaml").expect_err("bad ext");
        assert!(err.to_string().contains("Unsupported format"));

        let err = parse_format_from_args(None, "toml").expect_err("bad fallback");
        assert!(err.to_string().contains("Unsupported format"));
    }
}
