use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use rkyv::Deserialize as _;
use rkyv::check_archived_root;
use rkyv::ser::{Serializer, serializers::AllocSerializer};
use std::fs;
use std::path::PathBuf;

use pavis_core as binary;
use pavis_core::config as yaml;

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
    let archived = check_archived_root::<binary::ProxyConfig>(payload)
        .map_err(|e| anyhow!("Binary integrity check failed: {:?}", e))?;

    let binary_config: binary::ProxyConfig = archived.deserialize(&mut rkyv::Infallible)?;
    let yaml_config = convert_binary_to_config(binary_config);
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

fn convert_binary_to_config(binary: binary::ProxyConfig) -> yaml::Config {
    use pavis_core::{LoadBalancer, MatchType};

    let mut upstreams = Vec::new();
    for u in binary.upstreams {
        let lb = match u.load_balancer {
            LoadBalancer::Random => yaml::LoadBalancer::Random,
            LoadBalancer::RoundRobin => yaml::LoadBalancer::RoundRobin,
        };

        let mut endpoints = Vec::new();
        for e in u.endpoints {
            endpoints.push(yaml::Endpoint {
                ip: e.ip,
                port: e.port,
                weight: Some(e.weight),
                address: String::new(), // Will be ignored or recomputed
            });
        }

        upstreams.push(yaml::Upstream {
            name: u.name,
            load_balancer: lb,
            http_version: yaml::HttpVersion::H1,
            connection_pool: yaml::ConnectionPoolConfig::default(),
            tls: None,
            circuit_breaker: None,
            health_check: None,
            endpoints,
        });
    }

    let mut routes = Vec::new();
    for v in binary.routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let match_type = match p.match_type {
                MatchType::Exact => yaml::MatchType::Exact,
                MatchType::Regex => yaml::MatchType::Regex,
                MatchType::Prefix => yaml::MatchType::Prefix,
            };

            let request_headers = p.request_headers.map(|h| yaml::HeaderOperations {
                add: Some(h.add.into_iter().collect()),
                remove: Some(h.remove),
            });

            let response_headers = p.response_headers.map(|h| yaml::HeaderOperations {
                add: Some(h.add.into_iter().collect()),
                remove: Some(h.remove),
            });

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| yaml::WeightedDestination {
                    upstream: d.upstream,
                    weight: d.weight,
                })
                .collect();

            paths.push(yaml::Route {
                match_type,
                path: p.path,
                timeout: None,
                retry: None,
                request_headers,
                response_headers,
                destinations,
                compiled_regex: None,
            });
        }

        routes.push(yaml::VirtualHost {
            host: v.host,
            paths,
        });
    }

    yaml::Config {
        server: yaml::ServerConfig {
            listen_addr: binary.listen_addr,
            worker_threads: None,
            tls: None,
        },
        telemetry: yaml::TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: yaml::AccessLogConfig::False,
            tracing: None,
        },
        upstreams,
        routes,
    }
}

fn validate_yaml(input_path: PathBuf) -> Result<()> {
    let content = fs::read_to_string(&input_path).context("Failed to read input file")?;
    let yaml_config: yaml::Config =
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
    let yaml_config: yaml::Config =
        serde_yaml::from_str(&content).context("Failed to parse YAML")?;
    yaml_config
        .clone()
        .validate()
        .context("Invalid configuration")?;

    // 2. Convert to binary protocol structs
    let pavis_config = convert_to_pavis(yaml_config)?;

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
    let archived = check_archived_root::<binary::ProxyConfig>(payload)
        .map_err(|e| anyhow!("Binary integrity check failed: {:?}", e))?;

    println!("--- Config Tree ---");
    println!("Listen Address: {}", archived.listen_addr);
    println!("Upstreams ({}):", archived.upstreams.len());
    for u in archived.upstreams.iter() {
        let lb_str = match u.load_balancer {
            binary::ArchivedLoadBalancer::RoundRobin => "RoundRobin",
            binary::ArchivedLoadBalancer::Random => "Random",
        };
        println!("  - Name: {}", u.name);
        println!("    LB: {}", lb_str);
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

fn convert_to_pavis(src: yaml::Config) -> Result<binary::ProxyConfig> {
    let mut upstreams = Vec::new();
    for u in src.upstreams {
        let lb = match u.load_balancer {
            yaml::LoadBalancer::Random => binary::LoadBalancer::Random,
            yaml::LoadBalancer::RoundRobin => binary::LoadBalancer::RoundRobin,
        };

        let mut endpoints = Vec::new();
        for e in u.endpoints {
            endpoints.push(binary::Endpoint {
                ip: e.ip,
                port: e.port,
                weight: e.weight.unwrap_or(1),
            });
        }

        upstreams.push(binary::Upstream {
            name: u.name,
            load_balancer: lb,
            endpoints,
        });
    }

    let mut routes = Vec::new();
    for v in src.routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let match_type = match p.match_type {
                yaml::MatchType::Exact => binary::MatchType::Exact,
                yaml::MatchType::Regex => binary::MatchType::Regex,
                yaml::MatchType::Prefix => binary::MatchType::Prefix,
            };

            let request_headers = if let Some(h) = p.request_headers {
                let add: Vec<(String, String)> = h.add.unwrap_or_default().into_iter().collect();
                let remove = h.remove.unwrap_or_default();
                Some(binary::HeaderOperations { add, remove })
            } else {
                None
            };

            let response_headers = if let Some(h) = p.response_headers {
                let add: Vec<(String, String)> = h.add.unwrap_or_default().into_iter().collect();
                let remove = h.remove.unwrap_or_default();
                Some(binary::HeaderOperations { add, remove })
            } else {
                None
            };

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| binary::WeightedDestination {
                    upstream: d.upstream,
                    weight: d.weight,
                })
                .collect();

            paths.push(binary::Route {
                match_type,
                path: p.path,
                request_headers,
                response_headers,
                destinations,
            });
        }

        routes.push(binary::VirtualHost {
            host: v.host,
            paths,
        });
    }

    Ok(binary::ProxyConfig {
        header: binary::PavisHeader::default(),
        listen_addr: src.server.listen_addr,
        upstreams,
        routes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::config as yaml;
    use pavis_core::{Endpoint, LoadBalancer, PavisHeader, ProxyConfig, Upstream};

    #[test]
    fn test_convert_binary_to_config() {
        let binary = ProxyConfig {
            header: PavisHeader::default(),
            listen_addr: "0.0.0.0:8080".to_string(),
            upstreams: vec![Upstream {
                name: "test".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                endpoints: vec![Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 80,
                    weight: 1,
                }],
            }],
            routes: vec![],
        };

        let config = convert_binary_to_config(binary);

        assert_eq!(config.server.listen_addr, "0.0.0.0:8080");
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.upstreams[0].name, "test");
        assert_eq!(config.upstreams[0].endpoints[0].ip, "127.0.0.1");
        assert_eq!(
            config.upstreams[0].load_balancer,
            yaml::LoadBalancer::RoundRobin
        );
    }
}
