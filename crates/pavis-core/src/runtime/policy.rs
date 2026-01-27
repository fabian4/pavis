use crate::runtime::types::{Duration, HeaderName, HeaderValue};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
#[non_exhaustive]
pub enum HeadersPolicy {
    Disabled,
    Enabled { rules: Headers },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Headers {
    pub set_headers: Vec<(HeaderName, HeaderValue)>,
    pub append_headers: Vec<(HeaderName, HeaderValue)>,
    pub add_headers: Vec<(HeaderName, HeaderValue)>,
    pub remove_headers: Vec<HeaderName>,
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
#[repr(u8)]
#[non_exhaustive]
pub enum ReadTimeout {
    Disabled,
    Enabled(Duration),
}

impl Default for ReadTimeout {
    fn default() -> Self {
        Self::Enabled(Duration(std::num::NonZeroU32::new(30000).unwrap()))
    }
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
#[repr(u8)]
#[non_exhaustive]
pub enum ShutdownPolicy {
    Disabled,
    Enabled { drain_timeout: Duration },
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
#[repr(u8)]
#[non_exhaustive]
pub enum AdminConfig {
    Disabled,
    Enabled { addr: SocketAddr },
}

#[derive(
    Debug, Clone, PartialEq, Eq, Archive, RkyvDeserialize, RkyvSerialize, bytecheck::CheckBytes,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RegexLimits {
    pub pattern_max_bytes: u32,
    pub size_limit_bytes: u64,
    pub input_max_bytes: u32,
    pub max_regex_per_route: u32,
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
pub struct RoutingFeatures {
    #[cfg_attr(feature = "serde", serde(default))]
    pub advanced_matchers: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub regex_limits: RegexLimits,
}
