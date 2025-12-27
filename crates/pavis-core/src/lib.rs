use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

pub mod config;

/// Magic Bytes "PAVS" (Pavilion) to identify valid Pavis Core files.
pub const PAVIS_MAGIC: &[u8; 4] = b"PAVS";

/// Current Protocol Version. Increment this when breaking changes occur.
pub const PAVIS_VERSION: u32 = 2;

/// The Header of a Pavis configuration file.
/// Always present at the beginning of the binary blob.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone, Copy)]
#[archive(check_bytes)]
pub struct PavisHeader {
    pub magic: [u8; 4],
    pub version: u32,
}

impl Default for PavisHeader {
    fn default() -> Self {
        Self {
            magic: *PAVIS_MAGIC,
            version: PAVIS_VERSION,
        }
    }
}

/// The Root Configuration Object.
impl ProxyConfig {
    /// Convert binary protocol config back to YAML-compatible Config DTO.
    /// Useful for loading .pvs files into the runtime that still expects Config DTO.
    pub fn to_config(&self) -> config::Config {
        let mut upstreams = Vec::new();
        for u in &self.upstreams {
            let lb = match u.load_balancer {
                LoadBalancer::Random => config::LoadBalancer::Random,
                LoadBalancer::RoundRobin => config::LoadBalancer::RoundRobin,
            };

            let mut endpoints = Vec::new();
            for e in &u.endpoints {
                endpoints.push(config::Endpoint {
                    ip: e.ip.clone(),
                    port: e.port,
                    weight: Some(e.weight),
                    address: String::new(), // Will be pre-computed in validate()
                });
            }

            upstreams.push(config::Upstream {
                name: u.name.clone(),
                load_balancer: lb,
                http_version: config::HttpVersion::H1, // Defaulting as binary doesn't store this yet
                connection_pool: config::ConnectionPoolConfig::default(),
                tls: None,
                circuit_breaker: None,
                health_check: None,
                endpoints,
            });
        }

        let mut routes = Vec::new();
        for v in &self.routes {
            let mut paths = Vec::new();
            for p in &v.paths {
                let match_type = match p.match_type {
                    MatchType::Exact => config::MatchType::Exact,
                    MatchType::Regex => config::MatchType::Regex,
                    MatchType::Prefix => config::MatchType::Prefix,
                };

                let request_headers =
                    p.request_headers
                        .as_ref()
                        .map(|h| config::HeaderOperations {
                            add: Some(h.add.iter().cloned().collect()),
                            remove: Some(h.remove.clone()),
                        });

                let response_headers =
                    p.response_headers
                        .as_ref()
                        .map(|h| config::HeaderOperations {
                            add: Some(h.add.iter().cloned().collect()),
                            remove: Some(h.remove.clone()),
                        });

                let destinations = p
                    .destinations
                    .iter()
                    .map(|d| config::WeightedDestination {
                        upstream: d.upstream.clone(),
                        weight: d.weight,
                    })
                    .collect();

                paths.push(config::Route {
                    match_type,
                    path: p.path.clone(),
                    timeout: None,
                    retry: None,
                    request_headers,
                    response_headers,
                    destinations,
                    compiled_regex: None,
                });
            }

            routes.push(config::VirtualHost {
                host: v.host.clone(),
                paths,
            });
        }

        config::Config {
            server: config::ServerConfig {
                listen_addr: self.listen_addr.clone(),
                worker_threads: None,
                tls: None,
            },
            telemetry: config::TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: config::AccessLogConfig::False,
                tracing: None,
            },
            upstreams,
            routes,
        }
    }
}

/// The Root Configuration Object.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct ProxyConfig {
    pub header: PavisHeader,
    pub listen_addr: String,
    pub upstreams: Vec<Upstream>,
    pub routes: Vec<VirtualHost>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Upstream {
    pub name: String,
    pub load_balancer: LoadBalancer,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub enum LoadBalancer {
    RoundRobin,
    Random,
    // Add others as needed (e.g., LeastConnection)
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Endpoint {
    pub ip: String,
    pub port: u16,
    pub weight: u32,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct VirtualHost {
    pub host: String, // e.g. "example.com" or "*"
    pub paths: Vec<Route>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Route {
    pub match_type: MatchType,
    pub path: String,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub enum MatchType {
    Prefix,
    Exact,
    Regex,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct HeaderOperations {
    // Maps of HeaderName -> HeaderValue
    pub add: Vec<(String, String)>,
    pub remove: Vec<String>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Serialize, Deserialize, Debug, Clone)]
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

    fn create_valid_config() -> ProxyConfig {
        ProxyConfig {
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
        let result = check_archived_root::<ProxyConfig>(&bytes);
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
        let result = check_archived_root::<ProxyConfig>(&bytes);
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
        let result = check_archived_root::<ProxyConfig>(truncated_bytes);
        assert!(
            result.is_err(),
            "Validation should have failed for truncated data"
        );
    }

    #[test]
    fn test_version_mismatch_check() {
        let mut config = create_valid_config();
        config.header.version = 999; // Future version

        let mut serializer = AllocSerializer::<1024>::default();
        serializer.serialize_value(&config).unwrap();
        let bytes = serializer.into_serializer().into_inner();

        // rkyv validation checks structural integrity, not our logical version
        let archived = check_archived_root::<ProxyConfig>(&bytes).unwrap();

        // We should manually check the version
        assert_eq!(archived.header.version, 999);
        assert_ne!(archived.header.version, PAVIS_VERSION);
    }
}
