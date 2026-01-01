use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[archive(check_bytes)]
pub enum LoadBalancer {
    RoundRobin,
    #[default]
    Random,
    // Add others as needed (e.g., LeastConnection)
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct ConnectionPoolConfig {
    pub idle_timeout_secs: u64,
    pub connection_timeout_secs: u64,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct UpstreamTlsConfig {
    pub enabled: bool,
    pub verify_hostname: bool,
    pub verify_cert: bool,
    pub sni: Option<String>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Endpoint {
    pub ip: IpAddr,
    pub port: u16,
    pub weight: u32,
}
