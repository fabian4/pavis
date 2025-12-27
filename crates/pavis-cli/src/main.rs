use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use rkyv::Deserialize as _;
use rkyv::check_archived_root;
use rkyv::ser::{Serializer, serializers::AllocSerializer};
use std::fs;
use std::path::PathBuf;

use pavis_adapter_yaml::config as yaml;
use pavis_core as binary;

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
    let bytes = fs::read(&input_path).context("Failed to read input file")?;
    if bytes.len() < 8 {
        return Err(anyhow!("File too small"));
    }

    let payload = &bytes[8..];
    let archived = check_archived_root::<binary::RuntimeConfig>(payload)
        .map_err(|e| anyhow!("Binary integrity check failed: {:?}", e))?;

    let binary_config: binary::RuntimeConfig = archived.deserialize(&mut rkyv::Infallible)?;
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
    let content = fs::read_to_string(&input_path).context("Failed to read input file")?;
    let yaml_config: yaml::YamlConfig =
        serde_yaml::from_str(&content).context("Failed to parse YAML")?;
    yaml_config
        .validate()
        .context("Configuration validation failed")?;
    println!("✅ Configuration is valid: {:?}", input_path);
    Ok(())
}

fn compile_config(input_path: PathBuf, output_path: PathBuf) -> Result<()> {
    tracing::info!("Reading YAML config from {:?}", input_path);
    let content = fs::read_to_string(&input_path).context("Failed to read input file")?;

    // 1. Deserialize and Validate YAML
    let yaml_config: yaml::YamlConfig =
        serde_yaml::from_str(&content).context("Failed to parse YAML")?;
    yaml_config
        .clone()
        .validate()
        .context("Invalid configuration")?;

    // 2. Convert to binary protocol structs
    let pavis_config: binary::RuntimeConfig = yaml_config.try_into()?;

    // 3. Serialize to Bytes
    let mut serializer = AllocSerializer::<1024>::default();
    serializer
        .serialize_value(&pavis_config)
        .context("Failed to serialize to Pavis")?;
    let rkyv_bytes = serializer.into_serializer().into_inner();

    // 4. Write to Disk with explicit header
    let mut final_bytes = Vec::with_capacity(rkyv_bytes.len() + 8);
    final_bytes.extend_from_slice(binary::PAVIS_MAGIC);
    final_bytes.extend_from_slice(&binary::PAVIS_VERSION.to_le_bytes());
    final_bytes.extend_from_slice(&rkyv_bytes);

    fs::write(&output_path, final_bytes).context("Failed to write output file")?;

    let metadata = fs::metadata(&output_path)?;
    println!("✅ Successfully compiled config to {:?}", output_path);
    println!("   Size: {} bytes", metadata.len());
    println!("   Protocol Version: {}", binary::PAVIS_VERSION);

    Ok(())
}

fn inspect_config(input_path: PathBuf, hex: bool) -> Result<()> {
    let bytes = fs::read(&input_path).context("Failed to read input file")?;

    if bytes.len() < 8 {
        return Err(anyhow!("File too small to be a valid Pavis config"));
    }

    // Check Header manually first for better error messages
    let magic = &bytes[0..4];
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());

    println!("--- Pavis Header ---");
    println!("Magic: {:?}", std::str::from_utf8(magic).unwrap_or("????"));
    println!("Version: {}", version);
    println!();

    if magic != binary::PAVIS_MAGIC {
        return Err(anyhow!(
            "Invalid magic bytes. Expected 'PAVS', found {:?}",
            std::str::from_utf8(magic)
        ));
    }

    // Validate structural integrity with check_bytes (skip our 8-byte header)
    let payload = &bytes[8..];
    let archived = check_archived_root::<binary::RuntimeConfig>(payload)
        .map_err(|e| anyhow!("Binary integrity check failed: {:?}", e))?;

    println!("--- Config Tree ---");
    println!("Listen Address: {}", archived.listen_addr);
    println!("Upstreams ({}):", archived.upstreams.len());
    for u in archived.upstreams.iter() {
        let lb_str = match u.load_balancer {
            binary::ArchivedLoadBalancer::RoundRobin => "RoundRobin",
            binary::ArchivedLoadBalancer::Random => "Random",
        };
        let hv_str = match u.http_version {
            binary::ArchivedHttpVersion::H1 => "H1",
            binary::ArchivedHttpVersion::H2 => "H2",
            binary::ArchivedHttpVersion::H2H1 => "H2H1",
        };
        println!("  - Name: {}", u.name);
        println!("    LB: {}", lb_str);
        println!("    HTTP: {}", hv_str);
        if let rkyv::option::ArchivedOption::Some(tls) = &u.tls {
            println!("    TLS: Enabled (SNI: {:?})", tls.sni);
        } else {
            println!("    TLS: Disabled");
        }
        println!("    Endpoints ({}):", u.endpoints.len());
        for e in u.endpoints.iter() {
            println!("      - {}:{} (weight: {})", e.ip, e.port, e.weight);
        }
    }

    println!("Virtual Hosts ({}):", archived.routes.len());
    for v in archived.routes.iter() {
        println!("  - Host: {}", v.host);
        for p in v.paths.iter() {
            let mt_str = match p.match_type {
                binary::ArchivedMatchType::Prefix => "Prefix",
                binary::ArchivedMatchType::Exact => "Exact",
                binary::ArchivedMatchType::Regex => "Regex",
            };
            println!("    Path: {} ({})", p.path, mt_str);
            for d in p.destinations.iter() {
                println!("      -> {} (weight: {})", d.upstream, d.weight);
            }
        }
    }

    if hex {
        println!("\n--- Hex Dump (Payload) ---");
        // Simple hex dump of the payload part
        for (i, chunk) in bytes[8..].chunks(16).enumerate() {
            print!("{:08x}: ", i * 16);
            for b in chunk {
                print!("{:02x} ", b);
            }
            println!();
        }
    }

    Ok(())
}
