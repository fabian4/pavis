use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rkyv::ser::{Serializer, serializers::AllocSerializer};
use std::fs;
use std::path::PathBuf;

// We need temporary structs to deserialize the YAML before converting to Pavis.
// Ideally, we would share these with Pavis, but Pavis is moving to Pavis-Core-only loading.
// So Pavis xDS owns the "Source" (YAML) definition now.
mod yaml_model {
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Debug, Deserialize)]
    pub struct Config {
        pub server: ServerConfig,
        pub upstreams: Vec<Upstream>,
        pub routes: Vec<VirtualHost>,
    }

    #[derive(Debug, Deserialize)]
    pub struct ServerConfig {
        pub listen_addr: String,
    }

    #[derive(Debug, Deserialize)]
    pub struct Upstream {
        pub name: String,
        pub load_balancer: Option<String>,
        pub endpoints: Vec<Endpoint>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Endpoint {
        pub ip: String,
        pub port: u16,
        pub weight: Option<u32>,
    }

    #[derive(Debug, Deserialize)]
    pub struct VirtualHost {
        pub host: String,
        pub paths: Vec<Route>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Route {
        pub match_type: String,
        pub path: String,
        pub headers: Option<HeaderOperations>,
        pub request_headers: Option<HeaderOperations>,
        pub response_headers: Option<HeaderOperations>,
        pub destinations: Vec<WeightedDestination>,
    }

    #[derive(Debug, Deserialize)]
    pub struct HeaderOperations {
        pub add: Option<HashMap<String, String>>,
        pub remove: Option<Vec<String>>,
    }

    #[derive(Debug, Deserialize)]
    pub struct WeightedDestination {
        pub upstream: String,
        pub weight: u32,
    }
}

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
    let yaml_config: yaml_model::Config =
        serde_yaml::from_str(&content).context("Failed to parse YAML")?;

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

fn convert_to_pavis(src: yaml_model::Config) -> Result<pavis_core::ProxyConfig> {
    let mut upstreams = Vec::new();
    for u in src.upstreams {
        let lb = match u.load_balancer.as_deref() {
            Some("random") => pavis_core::LoadBalancer::Random,
            _ => pavis_core::LoadBalancer::RoundRobin, // Default
        };

        let mut endpoints = Vec::new();
        for e in u.endpoints {
            endpoints.push(pavis_core::Endpoint {
                ip: e.ip,
                port: e.port,
                weight: e.weight.unwrap_or(1),
            });
        }

        upstreams.push(pavis_core::Upstream {
            name: u.name,
            load_balancer: lb,
            endpoints,
        });
    }

    let mut routes = Vec::new();
    for v in src.routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let match_type = match p.match_type.as_str() {
                "exact" => pavis_core::MatchType::Exact,
                "regex" => pavis_core::MatchType::Regex,
                _ => pavis_core::MatchType::Prefix,
            };

            let request_headers = if let Some(h) = p.request_headers.or(p.headers) {
                let add: Vec<(String, String)> = h.add.unwrap_or_default().into_iter().collect();
                let remove = h.remove.unwrap_or_default();
                Some(pavis_core::HeaderOperations { add, remove })
            } else {
                None
            };

            let response_headers = if let Some(h) = p.response_headers {
                let add: Vec<(String, String)> = h.add.unwrap_or_default().into_iter().collect();
                let remove = h.remove.unwrap_or_default();
                Some(pavis_core::HeaderOperations { add, remove })
            } else {
                None
            };

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| pavis_core::WeightedDestination {
                    upstream: d.upstream,
                    weight: d.weight,
                })
                .collect();

            paths.push(pavis_core::Route {
                match_type,
                path: p.path,
                request_headers,
                response_headers,
                destinations,
            });
        }

        routes.push(pavis_core::VirtualHost {
            host: v.host,
            paths,
        });
    }

    Ok(pavis_core::ProxyConfig {
        header: pavis_core::PavisHeader::default(),
        listen_addr: src.server.listen_addr,
        upstreams,
        routes,
    })
}
