use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct AegisConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub worker_threads: Option<usize>,
    pub tls: TlsConfig,
}

#[derive(Debug, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelemetryConfig {
    pub service_name: String,
    pub prometheus_addr: String,
    pub access_log: String,
    pub pingora_log: bool,
    pub tracing: TracingConfig,
}

#[derive(Debug, Deserialize)]
pub struct TracingConfig {
    pub enabled: bool,
    pub provider: String,
    pub sampling_rate: f64,
}

#[derive(Debug, Deserialize)]
pub struct Upstream {
    pub name: String,
    pub load_balancer: String,
    pub circuit_breaker: Option<CircuitBreaker>,
    pub health_check: Option<HealthCheck>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize)]
pub struct CircuitBreaker {
    pub max_connections: usize,
    pub max_pending_requests: usize,
    pub max_retries: usize,
}

#[derive(Debug, Deserialize)]
pub struct HealthCheck {
    pub path: String,
    pub interval: String,
    pub timeout: Option<String>,
    pub unhealthy_threshold: Option<usize>,
    pub healthy_threshold: Option<usize>,
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
    pub timeout_ms: Option<u64>,
    pub retry: Option<RetryPolicy>,
    pub headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
}

#[derive(Debug, Deserialize)]
pub struct RetryPolicy {
    pub attempts: usize,
    // Using serde_yaml::Value or similar for mixed types might be needed if retry_on contains strings and ints,
    // but typically retry_on are status codes or strings. Let's use a custom deserializer or just Value for now
    // to match the "500, 502, 'gateway_error'" mix.
    // For simplicity here, assuming user input is consistent or we use a more flexible type.
    // The example showed [500, 502, 503, "gateway_error"...].
    // Let's use serde_yaml::Value to be safe.
    pub retry_on: Vec<serde_yaml::Value>,
    pub per_try_timeout_ms: u64,
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
