use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchType {
    #[default]
    Prefix,
    Exact,
    Regex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadBalancer {
    #[default]
    Random,
    RoundRobin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpVersion {
    #[default]
    H1,
    H2,
    H2H1,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub worker_threads: Option<usize>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AccessLogConfig {
    #[default]
    False,
    Stdout,
    File(String),
}

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub level: Option<String>,
    pub pingora: Option<String>,
    pub service_name: Option<String>,
    pub prometheus_addr: Option<String>,
    pub access_log: AccessLogConfig,
    pub tracing: Option<TracingConfig>,
}

#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub enabled: bool,
    pub provider: String,
    pub sampling_rate: f64,
}

#[derive(Debug, Clone)]
pub struct Upstream {
    pub name: String,
    pub load_balancer: LoadBalancer,
    pub http_version: HttpVersion,
    pub connection_pool: ConnectionPoolConfig,
    pub tls: Option<UpstreamTlsConfig>,
    pub circuit_breaker: Option<CircuitBreaker>,
    pub health_check: Option<HealthCheck>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Clone)]
pub struct UpstreamTlsConfig {
    pub enabled: bool,
    pub verify_hostname: Option<bool>,
    pub verify_cert: Option<bool>,
    pub sni: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    pub idle_timeout: Duration,
    pub connection_timeout: Duration,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(60),
            connection_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub max_connections: usize,
    pub max_pending_requests: usize,
    pub max_retries: usize,
}

#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub path: String,
    pub interval: Duration,
    pub timeout: Option<Duration>,
    pub unhealthy_threshold: Option<usize>,
    pub healthy_threshold: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub ip: String,
    pub port: u16,
    pub weight: Option<u32>,
}

impl Endpoint {
    pub fn address(&self) -> String {
        pavis_core::format_address(&self.ip, self.port)
    }
}

#[derive(Debug, Clone)]
pub struct VirtualHost {
    pub host: String,
    pub paths: Vec<Route>,
}

#[derive(Debug, Clone)]
pub struct Route {
    pub match_type: MatchType,
    pub path: String,
    pub timeout: Option<Duration>,
    pub retry: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
    pub compiled_regex: Option<regex::Regex>,
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub attempts: usize,
    // Using String for simplicity as we don't have serde_json::Value here
    pub retry_on: Vec<String>,
    pub per_try_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct HeaderOperations {
    pub add: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}
