use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

use pavis_adapter_yaml::config as yaml;
use pavis_core::{self as binary, Config, ConfigSource};
use pavis_pvs as pvs;

#[derive(Parser)]
#[command(name = "pavis-cli")]
#[command(version, about = "The developer tool for Pavis", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a YAML config file into a Pavis binary (.pvs)
    Compile {
        /// Input YAML file
        #[arg(short, long)]
        input: PathBuf,

        /// Output Pavis file
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Inspect a Pavis binary file (.pvs)
    Inspect {
        /// Input Pavis file
        #[arg(short, long)]
        input: PathBuf,

        /// Show hex dump of the payload
        #[arg(short = 'x', long)]
        hex: bool,
    },
    /// Validate a YAML config file without compiling
    Validate {
        /// Input YAML file
        #[arg(short, long)]
        input: PathBuf,
    },
    /// Convert a Pavis binary file (.pvs) back to YAML
    Convert {
        /// Input Pavis file
        #[arg(short, long)]
        input: PathBuf,

        /// Output YAML file (optional, prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile { input, output } => compile_config(input, output),
        Commands::Inspect { input, hex } => inspect_config(input, hex),
        Commands::Validate { input } => validate_yaml(input),
        Commands::Convert { input, output } => convert_to_yaml(input, output),
    }
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
    let yaml_config = yaml::YamlConfig::load(ConfigSource::File(&input_path))
        .context("Failed to load YAML config")?;
    Config::validate(&yaml_config).context("Configuration validation failed")?;
    let _runtime: binary::RuntimeConfig = yaml_config.build()?;
    println!("✅ Configuration is valid: {:?}", input_path);
    Ok(())
}

fn compile_config(input_path: PathBuf, output_path: PathBuf) -> Result<()> {
    tracing::info!("Reading YAML config from {:?}", input_path);
    let yaml_config = yaml::YamlConfig::load(ConfigSource::File(&input_path))
        .context("Failed to load YAML config")?;
    Config::validate(&yaml_config).context("Invalid configuration")?;

    // Convert to binary protocol structs
    let pavis_config: binary::RuntimeConfig = yaml_config.build()?;

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
    println!("--- Pavis Header ---");
    println!(
        "Magic: {:?}",
        std::str::from_utf8(&header.magic).unwrap_or("????")
    );
    println!("Version: {}", header.version);
    println!("Algorithm: {}", header.algorithm);
    println!("Checksum: {}", hex::encode(header.checksum));
    println!();

    let config = pvs::load(&input_path)?;

    println!("--- Config Tree ---");
    println!("Listen Address: {}", config.server.listen_addr);
    println!("Upstreams ({}):", config.upstreams.len());
    for u in &config.upstreams {
        let lb_str = match u.load_balancer {
            binary::LoadBalancer::RoundRobin => "RoundRobin",
            binary::LoadBalancer::Random => "Random",
        };
        let hv_str = match u.http_version {
            binary::HttpVersion::H1 => "H1",
            binary::HttpVersion::H2 => "H2",
            binary::HttpVersion::H2H1 => "H2H1",
        };
        println!(
            "- Upstream: {}, LB: {}, HTTP: {}, endpoints: {}",
            u.name,
            lb_str,
            hv_str,
            u.endpoints.len()
        );
        for ep in &u.endpoints {
            println!("  - {}:{} weight={}", ep.ip, ep.port, ep.weight);
        }
    }

    println!("Routes ({}):", config.routes.len());
    for vhost in &config.routes {
        println!("Host: {}", vhost.host);
        for route in &vhost.paths {
            let m = match route.match_type {
                binary::MatchType::Prefix => "prefix",
                binary::MatchType::Exact => "exact",
                binary::MatchType::Regex => "regex",
            };
            println!("  - [{}] {}", m, route.path);
            for dest in &route.destinations {
                println!("      -> {} (weight {})", dest.upstream, dest.weight);
            }
        }
    }

    if hex {
        let bytes = fs::read(&input_path).context("Failed to read input file")?;
        println!("--- Payload Hex Dump ---");
        let payload = &bytes[pvs::HEADER_SIZE..];
        println!("{}", hex::encode(payload));
    }

    Ok(())
}
