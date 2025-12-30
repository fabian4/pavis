use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

use super::HeaderOperations;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct VirtualHost {
    pub host: String, // e.g. "example.com" or "*"
    pub paths: Vec<Route>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct Route {
    pub match_type: MatchType,
    pub path: String,
    pub timeout_ms: Option<u64>,
    pub retry_policy: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub per_try_timeout_ms: u64,
    // Simple list of status codes or conditions expressed as strings.
    pub retry_on: Vec<String>,
}

#[derive(
    Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Default, Hash,
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[archive(check_bytes)]
pub enum MatchType {
    #[default]
    Prefix,
    Exact,
    Regex,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}
