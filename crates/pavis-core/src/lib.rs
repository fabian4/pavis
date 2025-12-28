use regex::Regex;
use rkyv::with::Skip;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use sha2::{Digest, Sha256};

/// Computes the SHA-256 checksum of the given payload.
pub fn compute_checksum(payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hasher.finalize().into()
}

/// Formats an IP address and port into a socket address string.
/// Handles IPv6 addresses by wrapping them in brackets if they don't already have them.
pub fn format_address(ip: &str, port: u16) -> String {
    if ip.contains(':') && !ip.starts_with('[') {
        format!("[{}]:{}", ip, port)
    } else {
        format!("{}:{}", ip, port)
    }
}

/// Magic Bytes "PAVS" (Pavilion) to identify valid Pavis Core files.
pub const PAVIS_MAGIC: &[u8; 4] = b"PAVS";

/// Current Protocol Version. Increment this when breaking changes occur.
pub const PAVIS_VERSION: u32 = 0;

/// Serialized header size in bytes.
pub const HEADER_SIZE: usize = 64;

/// The Header of a Pavis configuration file.
/// Always present at the beginning of the binary blob.
#[repr(C)]
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy)]
#[archive(check_bytes)]
pub struct PavisHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub algorithm: u32,
    pub checksum: [u8; 32],
    pub _reserved: [u8; 20],
}

impl Default for PavisHeader {
    fn default() -> Self {
        Self {
            magic: *PAVIS_MAGIC,
            version: PAVIS_VERSION,
            algorithm: 0,
            checksum: [0; 32],
            _reserved: [0; 20],
        }
    }
}

/// The Root Configuration Object.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct RuntimeConfig {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub worker_threads: Option<u64>, // usize in config.rs, u64 here for safety
    pub tls: Option<TlsConfig>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct TelemetryConfig {
    pub level: Option<String>,
    pub pingora: Option<String>,
    pub service_name: Option<String>,
    pub prometheus_addr: Option<String>,
    pub access_log: AccessLogConfig,
    pub tracing: Option<TracingConfig>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[archive(check_bytes)]
pub enum AccessLogConfig {
    False,
    #[default]
    Stdout,
    File(String),
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AccessLogConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Bool(bool),
            String(String),
        }

        match <Helper as serde::Deserialize>::deserialize(deserializer)? {
            Helper::Bool(false) => Ok(AccessLogConfig::False),
            Helper::Bool(true) => Err(serde::de::Error::custom("access_log cannot be true")),
            Helper::String(s) => match s.as_str() {
                "false" => Ok(AccessLogConfig::False),
                "stdout" => Ok(AccessLogConfig::Stdout),
                path if !path.is_empty() => Ok(AccessLogConfig::File(path.to_string())),
                _ => Err(serde::de::Error::custom(
                    "access_log must be 'false', 'stdout', or a file path",
                )),
            },
        }
    }
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct TracingConfig {
    pub enabled: bool,
    pub provider: String,
    pub sampling_rate: f64,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Upstream {
    pub name: String,
    pub load_balancer: LoadBalancer,
    pub http_version: HttpVersion,
    pub connection_pool: ConnectionPoolConfig,
    pub tls: Option<UpstreamTlsConfig>,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[archive(check_bytes)]
pub enum LoadBalancer {
    RoundRobin,
    #[default]
    Random,
    // Add others as needed (e.g., LeastConnection)
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[archive(check_bytes)]
pub enum HttpVersion {
    #[default]
    #[cfg_attr(feature = "serde", serde(alias = "1", alias = "1.1", alias = "http1"))]
    H1,
    #[cfg_attr(feature = "serde", serde(alias = "2", alias = "http2"))]
    H2,
    #[cfg_attr(feature = "serde", serde(alias = "auto"))]
    H2H1,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct ConnectionPoolConfig {
    pub idle_timeout_secs: u64,
    pub connection_timeout_secs: u64,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct UpstreamTlsConfig {
    pub enabled: bool,
    pub verify_hostname: bool,
    pub verify_cert: bool,
    pub sni: Option<String>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Endpoint {
    pub ip: String,
    pub port: u16,
    pub weight: u32,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct VirtualHost {
    pub host: String, // e.g. "example.com" or "*"
    pub paths: Vec<Route>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Route {
    pub match_type: MatchType,
    pub path: String,
    pub timeout_ms: Option<u64>,
    pub retry_policy: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
    #[with(Skip)]
    pub compiled_regex: Option<Regex>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub per_try_timeout_ms: u64,
    // Simple list of status codes or conditions?
    // For now let's stick to what was in pavis/config.rs: Vec<String>
    pub retry_on: Vec<String>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[archive(check_bytes)]
pub enum MatchType {
    #[default]
    Prefix,
    Exact,
    Regex,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct HeaderOperations {
    // Maps of HeaderName -> HeaderValue
    pub add: Vec<(String, String)>,
    pub remove: Vec<String>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rkyv::check_archived_root;
    use rkyv::ser::{Serializer, serializers::AllocSerializer};

    fn create_valid_config() -> RuntimeConfig {
        RuntimeConfig {
            server: ServerConfig {
                listen_addr: "0.0.0.0:8080".to_string(),
                worker_threads: None,
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::False,
                tracing: None,
            },
            upstreams: vec![Upstream {
                name: "test".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H1,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 60,
                    connection_timeout_secs: 5,
                },
                tls: None,
                endpoints: vec![Endpoint {
                    ip: "127.0.0.1".to_string(),
                    port: 80,
                    weight: 1,
                }],
            }],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Prefix,
                    path: "/".to_string(),
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "test".to_string(),
                        weight: 1,
                    }],
                    timeout_ms: None,
                    retry_policy: None,
                    compiled_regex: None,
                }],
            }],
        }
    }

    #[test]
    fn test_validation_valid_data() {
        let config = create_valid_config();
        let mut serializer = AllocSerializer::<1024>::default();
        serializer.serialize_value(&config).unwrap();
        let bytes = serializer.into_serializer().into_inner();

        // Should pass validation
        let result = check_archived_root::<RuntimeConfig>(&bytes);
        assert!(
            result.is_ok(),
            "Validation failed for valid data: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_validation_corrupted_data() {
        let config = create_valid_config();
        let mut serializer = AllocSerializer::<1024>::default();
        serializer.serialize_value(&config).unwrap();
        let mut bytes = serializer.into_serializer().into_inner().to_vec();

        // Corrupt some data in the middle
        if bytes.len() > 20 {
            for i in 10..20 {
                bytes[i] = 0xFF;
            }
        }

        // Should fail validation
        let result = check_archived_root::<RuntimeConfig>(&bytes);
        assert!(
            result.is_err(),
            "Validation should have failed for corrupted data"
        );
    }

    #[test]
    fn test_validation_truncated_data() {
        let config = create_valid_config();
        let mut serializer = AllocSerializer::<1024>::default();
        serializer.serialize_value(&config).unwrap();
        let bytes = serializer.into_serializer().into_inner();

        // Truncate the data
        let truncated_bytes = &bytes[..bytes.len() / 2];

        // Should fail validation
        let result = check_archived_root::<RuntimeConfig>(truncated_bytes);
        assert!(
            result.is_err(),
            "Validation should have failed for truncated data"
        );
    }

    #[test]
    fn test_format_address() {
        // IPv4
        assert_eq!(format_address("127.0.0.1", 8080), "127.0.0.1:8080");

        // IPv6 (without brackets) -> should add brackets
        assert_eq!(format_address("::1", 80), "[::1]:80");
        assert_eq!(format_address("2001:db8::1", 443), "[2001:db8::1]:443");

        // IPv6 (already has brackets) -> should keep brackets
        // Note: Our current logic doesn't explicitly strip and re-add,
        // it just checks starts_with('[').
        // If the input IP *string* has brackets (which is unusual for just the IP part,
        // but possible if misconfigured), it handles it safely by not double-bracketing.
        assert_eq!(format_address("[::1]", 80), "[::1]:80");

        // Hostname
        assert_eq!(format_address("example.com", 80), "example.com:80");
        assert_eq!(format_address("localhost", 3000), "localhost:3000");
    }
}
