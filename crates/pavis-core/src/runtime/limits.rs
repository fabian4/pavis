//! Regex limits and routing feature flags (P2)
//!
//! This module defines resource limits for regex matching and capability flags
//! for advanced routing features.

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Regex resource limits (enforced at runtime during config apply)
#[derive(
    Debug, Clone, PartialEq, Eq, Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
#[rkyv(attr(derive(Debug)))]
pub struct RegexLimits {
    /// Maximum pattern length in bytes (per regex)
    pub pattern_max_bytes: u32,

    /// Maximum compiled regex size in bytes (per regex)
    pub size_limit_bytes: u64,

    /// Maximum input length in bytes to match against (per request)
    pub input_max_bytes: u32,

    /// Maximum number of regex matchers per route
    pub max_regex_per_route: u32,

    /// Maximum total number of regex matchers across entire config
    pub max_regex_per_config: u32,
}

impl Default for RegexLimits {
    fn default() -> Self {
        Self {
            pattern_max_bytes: 256,
            size_limit_bytes: 10 * 1024 * 1024, // 10 MB
            input_max_bytes: 4096,
            max_regex_per_route: 10,
            max_regex_per_config: 100,
        }
    }
}

/// Routing feature flags (checked at runtime during config apply)
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Archive,
    RkyvDeserialize,
    RkyvSerialize,
    bytecheck::CheckBytes,
    Default,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[rkyv(compare(PartialEq))]
#[rkyv(attr(derive(Debug)))]
pub struct RoutingFeatures {
    /// Enable advanced matchers (header prefix/regex, logical operators)
    #[cfg_attr(feature = "serde", serde(default))]
    pub advanced_matchers: bool,

    /// Regex resource limits (only enforced if advanced_matchers is true)
    #[cfg_attr(feature = "serde", serde(default))]
    pub regex_limits: RegexLimits,
}
