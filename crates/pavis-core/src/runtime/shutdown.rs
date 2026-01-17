//! Graceful shutdown configuration.

use crate::runtime::types::Duration;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Shutdown policy for the runtime.
///
/// Controls how the proxy responds to termination signals (SIGTERM/SIGINT).
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum ShutdownPolicy {
    /// Exit immediately on signal without draining connections.
    Disabled,
    /// Gracefully drain in-flight requests before exiting.
    ///
    /// The runtime will:
    /// 1. Stop accepting new connections immediately
    /// 2. Wait for in-flight requests to complete (up to `drain_timeout`)
    /// 3. Force-close remaining connections after timeout
    /// 4. Clean up background services and exit
    Enabled { drain_timeout: Duration },
}
