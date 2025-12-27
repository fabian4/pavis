use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rkyv::ser::{Serializer, serializers::AllocSerializer};
use std::fs;
use std::path::PathBuf;

use pavis_core as binary;
use pavis_core::config as yaml; // User Input (YAML) // Binary Protocol (Rkyv)

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
    let yaml_config: yaml::RawConfig =
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
    let pavis_config = convert_to_pavis(yaml_config)?;

    // 3. Serialize to Bytes
    let mut serializer = AllocSerializer::<1024>::default();
    serializer
        .serialize_value(&pavis_config)
        .context("Failed to serialize to Pavis")?;
    let bytes = serializer.into_serializer().into_inner();

    // 4. Write to Disk
    fs::write(&output_path, bytes).context("Failed to write output file")?;
    tracing::info!(
        "Successfully compiled config to {:?} (Size: {} bytes)",
        output_path,
        fs::metadata(&output_path)?.len()
    );

    Ok(())
}

fn convert_to_pavis(src: yaml::RawConfig) -> Result<binary::WireConfig> {
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

        let http_version = match u.http_version {
            yaml::HttpVersion::H1 => binary::HttpVersion::H1,
            yaml::HttpVersion::H2 => binary::HttpVersion::H2,
            yaml::HttpVersion::H2H1 => binary::HttpVersion::H2H1,
        };

        let connection_pool = binary::ConnectionPoolConfig {
            idle_timeout_secs: u.connection_pool.idle_timeout.as_secs(),
            connection_timeout_secs: u.connection_pool.connection_timeout.as_secs(),
        };

        let tls = u.tls.map(|t| binary::UpstreamTlsConfig {
            enabled: t.enabled,
            verify_hostname: t.verify_hostname.unwrap_or(true),
            verify_cert: t.verify_cert.unwrap_or(true),
            sni: t.sni,
        });

        upstreams.push(binary::Upstream {
            name: u.name,
            load_balancer: lb,
            http_version,
            connection_pool,
            tls,
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

    Ok(binary::WireConfig {
        header: binary::PavisHeader::default(),
        listen_addr: src.server.listen_addr,
        upstreams,
        routes,
    })
}
