use crate::runtime::types::{
    ConnectTimeout, Hostname, IdleTimeout, Path, Port, UpstreamId, UpstreamName, Weight,
};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::num::NonZeroU32;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Upstream {
    pub id: UpstreamId,
    pub name: UpstreamName,
    pub discovery: Discovery,

    pub balancer: LoadBalancer,
    pub protocol: HttpVersion,
    pub pool: Pool,
    pub tls: TlsPolicy,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum Discovery {
    #[default]
    Static,
    Strict {
        ttl: u32,
    },
    Logical,
}

#[repr(u8)]
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum LoadBalancer {
    RoundRobin = 0,
    #[default]
    Random = 1,
    LeastRequest = 2,
}

#[repr(u8)]
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum HttpVersion {
    #[default]
    #[cfg_attr(feature = "serde", serde(alias = "1", alias = "1.1", alias = "http1"))]
    H1 = 0,
    #[cfg_attr(feature = "serde", serde(alias = "2", alias = "http2"))]
    H2 = 1,
    H2H1 = 2,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Pool {
    pub idle: IdleTimeout,
    pub connect: ConnectTimeout,
    pub max: ConnectionLimit,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum ConnectionLimit {
    Unlimited,
    Limited(NonZeroU32),
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum TlsPolicy {
    Disabled,
    Enabled {
        mode: TlsVerify,
        sni: SniName,
        cert: ClientCert,
    },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum ClientCert {
    Disabled,
    Enabled { cert_path: Path, key_path: Path },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum TlsVerify {
    Disabled,
    Cert,
    CertAndHost,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum SniName {
    Auto,
    Value(Hostname),
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Endpoint {
    pub address: EndpointAddr,
    pub weight: Weight,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum EndpointAddr {
    Ip { address: IpAddr, port: Port },
    Dns { host: Hostname, port: Port },
}
