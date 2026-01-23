use clap::Parser;
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

/// Configuration file format for upstream behavior.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct UpstreamConfigFile {
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub failure_sequence: Option<Vec<FailureRule>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FailureRule {
    pub attempt: u32,
    pub status: u16,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct UpstreamArgs {
    #[arg(long, env = "UPSTREAM_BIND_ADDR", default_value = "0.0.0.0")]
    pub bind_addr: IpAddr,

    /// Shorthand for setting both HTTP and HTTPS ports to the same value
    #[arg(long, env = "PORT")]
    pub port: Option<u16>,

    #[arg(long, env = "HTTP_PORT", default_value = "8080")]
    pub http_port: u16,

    #[arg(long, env = "HTTPS_PORT", default_value = "8443")]
    pub https_port: u16,

    #[arg(long, env = "INSTANCE_ID", default_value = "pavis-upstream")]
    pub instance_id: String,

    #[arg(long, env = "TLS_CERT_FILE")]
    pub cert_path: Option<PathBuf>,

    #[arg(long, env = "TLS_KEY_FILE")]
    pub key_path: Option<PathBuf>,

    /// Path to JSON config file (optional, overrides CLI args)
    #[arg(long, env = "UPSTREAM_CONFIG")]
    pub config: Option<PathBuf>,
}

impl UpstreamArgs {
    /// Resolve the final configuration by merging CLI args with config file.
    pub fn resolve(mut self) -> anyhow::Result<Self> {
        // Load config file if provided
        if let Some(config_path) = &self.config {
            let content = std::fs::read_to_string(config_path)
                .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;
            let config: UpstreamConfigFile = serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;

            // Config file overrides CLI args
            if let Some(instance_id) = config.instance_id {
                self.instance_id = instance_id;
            }
            // Note: delay_ms is stored for later use by the upstream server
        }

        // Apply --port shorthand if provided
        if let Some(port) = self.port {
            self.http_port = port;
            // Only set HTTPS port if TLS is configured
            if self.cert_path.is_some() && self.key_path.is_some() {
                self.https_port = port;
            }
        }

        Ok(self)
    }
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct RelayArgs {
    #[arg(long, env = "MOCK_RELAY_LISTEN", default_value = "0.0.0.0:8080")]
    pub listen: SocketAddr,

    #[arg(long, env = "MOCK_RELAY_TIMEOUT_MS", default_value_t = 30000)]
    pub default_timeout_ms: u64,

    #[arg(long, env = "MOCK_RELAY_MAX_BODY", default_value_t = 10485760)]
    pub max_body: usize,
}
