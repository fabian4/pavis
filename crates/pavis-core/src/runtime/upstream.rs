use crate::runtime::types::{
    ConnectTimeout, ConsecutiveErrors, Duration, Hostname, IdleTimeout, MaxConnections,
    MaxPendingRequests, Path, Port, UpstreamId, UpstreamName, Weight,
};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::num::NonZeroU32;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Upstream {
    pub id: UpstreamId,
    pub name: UpstreamName,
    pub discovery: Discovery,

    pub balancer: LoadBalancer,
    pub protocol: HttpVersion,
    pub pool: Pool,
    pub outlier_detection: OutlierDetectionPolicy,
    pub circuit_breaker: CircuitBreakerPolicy,
    pub health_check: ActiveHealthCheck,
    pub tls: TlsPolicy,
    pub endpoints: Vec<Endpoint>,
}

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[repr(u8)]
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
#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum LoadBalancer {
    RoundRobin = 0,
    #[default]
    Random = 1,
    LeastRequest = 2,
}

#[repr(u8)]
#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[non_exhaustive]
pub enum HttpVersion {
    #[default]
    #[cfg_attr(feature = "serde", serde(alias = "1", alias = "1.1", alias = "http1"))]
    H1 = 0,
    #[cfg_attr(feature = "serde", serde(alias = "2", alias = "http2"))]
    H2 = 1,
    H2H1 = 2,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Pool {
    pub idle: IdleTimeout,
    pub connect: ConnectTimeout,
    pub max: ConnectionLimit,
    pub queue: PoolQueue,

    /// TCP keepalive duration in milliseconds. None = use Pingora/OS default.
    /// Recommended: 60000ms (60s) for NAT/firewall traversal.
    pub tcp_keepalive: Option<Duration>,

    /// Enable TCP_NODELAY to disable Nagle's algorithm (lower latency).
    /// None = use Pingora default (true). Set to false only for bulk transfer scenarios.
    pub tcp_nodelay: Option<bool>,

    /// TCP receive buffer size in bytes. None = use OS default.
    /// Typical range: 64KB-512KB for high-throughput backends.
    pub recv_buffer_size: Option<u32>,
}

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PoolQueue {
    /// Maximum number of queued requests waiting for an upstream connection.
    /// A value of 0 disables queuing.
    pub capacity: u32,
    /// Maximum time (milliseconds) a request may wait in the queue before failing.
    pub timeout_ms: u32,
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            idle: IdleTimeout::Disabled,
            connect: ConnectTimeout::Disabled,
            max: ConnectionLimit(
                NonZeroU32::new(DEFAULT_POOL_MAX).expect("DEFAULT_POOL_MAX is non-zero"),
            ),
            queue: PoolQueue::default(),
            tcp_keepalive: None,
            tcp_nodelay: None,
            recv_buffer_size: None,
        }
    }
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
/// Passive outlier detection policy for upstream endpoints.
pub enum OutlierDetectionPolicy {
    Disabled,
    Enabled {
        consecutive_errors: ConsecutiveErrors,
        eject_duration: Duration,
    },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
/// Circuit breaker policy for upstream request limits.
pub enum CircuitBreakerPolicy {
    Disabled,
    Enabled {
        max_connections: MaxConnections,
        max_pending_requests: MaxPendingRequests,
    },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
/// Active health check policy for upstream endpoints.
pub enum ActiveHealthCheck {
    Disabled,
    Enabled {
        path: Path,
        interval: Duration,
        timeout: Duration,
    },
}

/// Maximum concurrent connections per upstream peer.
/// Per P0 plan: must always be >= 1 (no unlimited variant).
#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ConnectionLimit(pub NonZeroU32);

/// Default pool max connections (per P0 plan).
pub const DEFAULT_POOL_MAX: u32 = 128;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
pub enum TlsPolicy {
    Disabled,
    Enabled {
        verify: TlsVerify,
        sni: SniName,
        canonical_sni: CanonicalSni,
        reuse_across_sni: ReuseAcrossSni,
        cert: ClientCert,
        ca: UpstreamCa,
    },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[repr(u8)]
#[non_exhaustive]
pub enum CanonicalSni {
    Disabled,
    Enabled { name: Hostname },
}

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[repr(u8)]
#[non_exhaustive]
pub enum ReuseAcrossSni {
    Disabled,
    Enabled,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[repr(u8)]
#[non_exhaustive]
pub enum ClientCertChain {
    None,
    Embedded,
    File { path: Path },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
pub enum ClientCert {
    Disabled,
    Enabled {
        cert_path: Path,
        key_path: Path,
        chain: ClientCertChain,
    },
}

#[derive(
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[repr(u8)]
#[non_exhaustive]
pub enum TlsVerify {
    Disabled,
    CaOnly,
    Full,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[repr(u8)]
#[non_exhaustive]
pub enum SniName {
    Auto,
    Name(Hostname),
    Disabled,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[repr(u8)]
#[non_exhaustive]
pub enum UpstreamCa {
    System,
    File { path: Path },
}

#[derive(
    Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone, PartialEq, Eq,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Endpoint {
    pub address: EndpointAddr,
    pub weight: Weight,
}

#[derive(
    Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone, PartialEq, Eq,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
pub enum EndpointAddr {
    Ip { address: IpAddr, port: Port },
    Dns { host: Hostname, port: Port },
}
