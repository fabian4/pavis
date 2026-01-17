//! Admin API configuration.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use std::net::SocketAddr;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Admin API configuration.
///
/// The admin API provides read-only operational endpoints:
/// - `GET /health` - Health status (always returns 200 OK)
/// - `GET /stats` - Runtime statistics (version, uptime, config counts)
///
/// Security: Should bind to loopback (127.0.0.1) or Unix socket.
/// No authentication is provided in Phase 7.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum AdminConfig {
    /// Admin API is disabled.
    Disabled,
    /// Admin API is enabled and listening on the specified address.
    Enabled { addr: SocketAddr },
}
