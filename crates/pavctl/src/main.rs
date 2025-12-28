use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

use pavctl::{format_config, format_header, parse_yaml_runtime_from_source};
use pavis_adapter_yaml::config as yaml;
use pavis_core::ConfigSource;
use pavis_pvs as pvs;

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

fn get_default_output(input: &Path, new_ext: &str) -> PathBuf {
    let mut out = input.to_path_buf();
    out.set_extension(new_ext);
    out
}

fn convert_to_yaml(input_path: PathBuf, output_path: Option<PathBuf>) -> Result<()> {
    let binary_config = pvs::load(&input_path)?;
    let yaml_config: yaml::YamlConfig = binary_config.into();
    let yaml_str = serde_yaml::to_string(&yaml_config).context("Failed to serialize to YAML")?;

    match output_path {
        Some(path) => {
            fs::write(&path, yaml_str).context("Failed to write output file")?;
            println!(
                "✅ Successfully converted {:?} to YAML at {:?}",
                input_path, path
            );
        }
        None => {
            println!("{}", yaml_str);
        }
    }
    Ok(())
}

fn validate_yaml(input_path: PathBuf) -> Result<()> {
    let _runtime = parse_yaml_runtime_from_source(ConfigSource::File(input_path.as_path()))?;
    println!("✅ Configuration is valid: {:?}", input_path);
    Ok(())
}

fn compile_config(input_path: PathBuf, output_path: PathBuf) -> Result<()> {
    tracing::info!("Reading YAML config from {:?}", input_path);
    let pavis_config = parse_yaml_runtime_from_source(ConfigSource::File(input_path.as_path()))?;

    // 3. Write to Disk with explicit header
    pvs::write(&output_path, &pavis_config)?;

    let metadata = fs::metadata(&output_path)?;
    println!("✅ Successfully compiled config to {:?}", output_path);
    println!("   Size: {} bytes", metadata.len());
    println!("   Protocol Version: {}", pvs::PAVIS_VERSION);

    Ok(())
}

fn inspect_config(input_path: PathBuf, hex: bool) -> Result<()> {
    let header = pvs::read_header(&input_path)?;
    print!("{}", format_header(&header));

    let config = pvs::load(&input_path)?;

    print!("{}", format_config(&config));

    if hex {
        let bytes = fs::read(&input_path).context("Failed to read input file")?;
        println!("--- Payload Hex Dump ---");
        let payload = &bytes[pvs::HEADER_SIZE..];
        println!("{}", hex::encode(payload));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        compile_config, convert_to_yaml, get_default_output, inspect_config, validate_yaml,
    };
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

    fn write_yaml(path: &PathBuf, content: &str) {
        fs::write(path, content).expect("write yaml");
    }

    fn minimal_yaml() -> &'static str {
        r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
"#
    }

    #[test]
    fn test_default_output_logic() {
        let input = PathBuf::from("config.yaml");
        assert_eq!(
            get_default_output(&input, "pvs"),
            PathBuf::from("config.pvs")
        );

        let input2 = PathBuf::from("dir/test.pvs");
        assert_eq!(
            get_default_output(&input2, "yaml"),
            PathBuf::from("dir/test.yaml")
        );
    }

    #[test]
    fn generate_inspect_and_convert_workflow() {
        let yaml_path = unique_path("pavctl_test", "yaml");
        let pvs_path = unique_path("pavctl_test", "pvs");
        let out_yaml = unique_path("pavctl_out", "yaml");

        write_yaml(&yaml_path, minimal_yaml());

        compile_config(yaml_path.clone(), pvs_path.clone()).expect("compile");
        inspect_config(pvs_path.clone(), false).expect("inspect");
        convert_to_yaml(pvs_path.clone(), Some(out_yaml.clone())).expect("convert");
        validate_yaml(out_yaml.clone()).expect("validate output");

        let _ = fs::remove_file(&yaml_path);
        let _ = fs::remove_file(&pvs_path);
        let _ = fs::remove_file(&out_yaml);
    }

    #[test]
    fn validate_rejects_unknown_upstream() {
        let yaml_path = unique_path("pavctl_bad", "yaml");
        let content = r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        destinations:
          - upstream: "missing"
            weight: 1
"#;
        write_yaml(&yaml_path, content);

        let err = validate_yaml(yaml_path.clone()).expect_err("should fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("unknown upstream"));

        let _ = fs::remove_file(&yaml_path);
    }
}
