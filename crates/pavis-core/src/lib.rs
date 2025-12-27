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

/// Deserializes a .pvs binary buffer into a ProxyConfig.
/// Handles validation and deserialization.
///
/// # Handling `#[with(...)]`
/// 1. `deserialize()` returns `With<T, Adapter>` if `#[with(...)]` is used.
/// 2. Explicitly call `.into_inner()` to get `T`.
/// 3. This helper wraps that logic so the runtime doesn't deal with `With` directly.
pub fn deserialize_pvs(bytes: &[u8]) -> anyhow::Result<ProxyConfig> {
    use rkyv::Deserialize;

    // Validate the archive
    let archived = rkyv::check_archived_root::<ProxyConfig>(bytes)
        .map_err(|e| anyhow::anyhow!("Binary integrity check failed: {:?}", e))?;

    // Deserialize
    // Note: If ProxyConfig uses #[with(...)], we would need:
    // let wrapper = archived.deserialize(&mut rkyv::Infallible)?;
    // let config = wrapper.into_inner();
    let config: ProxyConfig = archived.deserialize(&mut rkyv::Infallible)?;

    Ok(config)
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
