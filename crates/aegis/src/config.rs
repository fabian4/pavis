#![allow(dead_code)]
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct AegisConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub worker_threads: Option<usize>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelemetryConfig {
    pub level: Option<String>,
    pub pingora: Option<String>,
    pub service_name: Option<String>,
    pub prometheus_addr: Option<String>,
    pub access_log: Option<String>,
    pub tracing: Option<TracingConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TracingConfig {
    pub enabled: bool,
    pub provider: String,
    pub sampling_rate: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Upstream {
    pub name: String,
    pub load_balancer: Option<String>,
    pub circuit_breaker: Option<CircuitBreaker>,
    pub health_check: Option<HealthCheck>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CircuitBreaker {
    pub max_connections: usize,
    pub max_pending_requests: usize,
    pub max_retries: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HealthCheck {
    pub path: String,
    pub interval: String,
    pub timeout: Option<String>,
    pub unhealthy_threshold: Option<usize>,
    pub healthy_threshold: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Endpoint {
    pub ip: String,
    pub port: u16,
    pub weight: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VirtualHost {
    pub host: String,
    pub paths: Vec<Route>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Route {
    pub match_type: String,
    pub path: String,
    pub timeout_ms: Option<u64>,
    pub retry: Option<RetryPolicy>,
    pub headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
pub struct HeaderOperations {
    pub add: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}
