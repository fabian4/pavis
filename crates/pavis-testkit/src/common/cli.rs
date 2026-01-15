use clap::Parser;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct UpstreamArgs {
    #[arg(long, env = "UPSTREAM_BIND_ADDR", default_value = "0.0.0.0")]
    pub bind_addr: IpAddr,

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
