use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

/// Magic Bytes "PAVS" (Pavilion) to identify valid Pavis Core files.
pub const PAVIS_MAGIC: &[u8; 4] = b"PAVS";

/// Current Protocol Version. Increment this when breaking changes occur.
pub const PAVIS_VERSION: u32 = 1;

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
    pub headers: Option<HeaderOperations>,
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
