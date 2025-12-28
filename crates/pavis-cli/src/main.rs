use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use rkyv::Deserialize as _;
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
    if bytes.len() < binary::HEADER_SIZE {
        return Err(anyhow!("File too small to contain a valid header"));
    }

    let magic = &bytes[0..4];
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let algorithm = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let checksum = &bytes[12..44];

    if magic != binary::PAVIS_MAGIC {
        return Err(anyhow!("Invalid magic bytes. Expected 'PAVS'"));
    }

    if version != binary::PAVIS_VERSION {
        return Err(anyhow!(
            "Version mismatch. File: {}, CLI supports: {}",
            version,
            binary::PAVIS_VERSION
        ));
    }

    if algorithm != 1 {
        return Err(anyhow!(
            "Unsupported hash algorithm: {} (only SHA-256 id=1 is supported)",
            algorithm
        ));
    }

    let payload = &bytes[binary::HEADER_SIZE..];
    let computed_checksum = binary::compute_checksum(payload);
    if computed_checksum != checksum {
        return Err(anyhow!("Checksum mismatch in input file"));
    }

    let archived = rkyv::check_archived_root::<binary::RuntimeConfig>(payload)
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
    let yaml_config = yaml::YamlConfig::parse_str(&content).context("Failed to parse YAML")?;
    let validated = yaml_config
        .validate()
        .context("Configuration validation failed")?;
    let _runtime: binary::RuntimeConfig = validated.try_into()?;
    println!("✅ Configuration is valid: {:?}", input_path);
    Ok(())
}

fn compile_config(input_path: PathBuf, output_path: PathBuf) -> Result<()> {
    tracing::info!("Reading YAML config from {:?}", input_path);
    let content = fs::read_to_string(&input_path).context("Failed to read input file")?;

    // 1. Deserialize and Validate YAML
    let yaml_config = yaml::YamlConfig::parse_str(&content).context("Failed to parse YAML")?;
    let validated = yaml_config.validate().context("Invalid configuration")?;

    // 2. Convert to binary protocol structs
    let pavis_config: binary::RuntimeConfig = validated.try_into()?;

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

    // We need to serialize the header manually or use rkyv?
    // PavisHeader is repr(C) so we can just write bytes.
    // But let's be safe and use a defined way.
    // Since it's repr(C) and simple types, we can cast to bytes safely if we are careful about endianness.
    // However, rkyv might add padding.
    // Let's just write fields manually to be endian-safe and consistent.

    let mut final_bytes = Vec::with_capacity(rkyv_bytes.len() + binary::HEADER_SIZE);
    final_bytes.extend_from_slice(&header.magic);
    final_bytes.extend_from_slice(&header.version.to_le_bytes());
    final_bytes.extend_from_slice(&header.algorithm.to_le_bytes());
    final_bytes.extend_from_slice(&header.checksum);
    final_bytes.extend_from_slice(&header._reserved);

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

    if bytes.len() < binary::HEADER_SIZE {
        return Err(anyhow!("File too small to be a valid Pavis config"));
    }

    let magic = &bytes[0..4];
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let algorithm = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let checksum = &bytes[12..44];

    println!("--- Pavis Header ---");
    println!("Magic: {:?}", std::str::from_utf8(magic).unwrap_or("????"));
    println!("Version: {}", version);
    println!("Algorithm: {}", algorithm);
    println!("Checksum: {}", hex::encode(checksum));
    println!();

    if magic != binary::PAVIS_MAGIC {
        return Err(anyhow!(
            "Invalid magic bytes. Expected 'PAVS', found {:?}",
            std::str::from_utf8(magic)
        ));
    }

    if algorithm != 1 {
        return Err(anyhow!(
            "Unsupported hash algorithm: {} (only SHA-256 id=1 is supported)",
            algorithm
        ));
    }

    let payload = &bytes[binary::HEADER_SIZE..];
    let computed_checksum = binary::compute_checksum(payload);
    if computed_checksum != checksum {
        return Err(anyhow!("Checksum mismatch in input file"));
    }

    let archived = rkyv::check_archived_root::<binary::RuntimeConfig>(payload)
        .map_err(|e| anyhow!("Binary integrity check failed: {:?}", e))?;
    let config: binary::RuntimeConfig = archived.deserialize(&mut rkyv::Infallible)?;

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
        println!("--- Payload Hex Dump ---");
        let payload = &bytes[binary::HEADER_SIZE..];
        println!("{}", hex::encode(payload));
    }

    Ok(())
}
