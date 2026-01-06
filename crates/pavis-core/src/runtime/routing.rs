use crate::runtime::HeadersPolicy;
use crate::runtime::types::{Host, Hostname, Path, Timeout, TryTimeout, UpstreamName, Weight};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::num::NonZeroU16;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct VirtualHost {
    pub host: Host,
    pub paths: Vec<Route>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Route {
    pub matcher: PathMatch,
    pub timeout: Timeout,
    pub retry: RetryPolicy,
    pub request_headers: HeadersPolicy,
    pub response_headers: HeadersPolicy,
    pub rewrite: Rewrite,
    pub action: RouteAction,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub enum RouteAction {
    Forward(Vec<Destination>),
    Redirect { status: u16, location: String },
    Direct { status: u16, body: String },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Rewrite {
    pub path: RewritePath,
    pub host: RewriteHost,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub enum RetryPolicy {
    Disabled,
    Enabled {
        attempts: NonZeroU16,
        per_try: TryTimeout,
        on: RetryFlags,
    },
}

#[repr(u8)]
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[archive(check_bytes)]
pub enum PathMatch {
    Prefix { path: Path },
    Exact { path: Path },
    Regex { path: Path },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct RetryFlags(pub u8);

#[allow(dead_code)]
pub const RETRY_FIVE_XX: u8 = 0b0000_0001;
#[allow(dead_code)]
pub const RETRY_CONNECT_FAILURE: u8 = 0b0000_0010;
#[allow(dead_code)]
pub const RETRY_RESET: u8 = 0b0000_0100;
#[allow(dead_code)]
pub const RETRY_REFUSED: u8 = 0b0000_1000;
#[allow(dead_code)]
pub const RETRY_RESERVED: u8 = 0b1111_0000;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Destination {
    pub upstream: UpstreamName,
    pub weight: Weight,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub enum RewritePath {
    Disabled,
    Prefix { from: Path, to: Path },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub enum RewriteHost {
    Disabled,
    Literal { host: Hostname },
}
