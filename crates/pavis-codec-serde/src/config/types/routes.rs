use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use pavis_core::MatchType;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VirtualHost {
    pub host: String,
    pub paths: Vec<Route>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Route {
    #[serde(default)]
    pub match_type: MatchType,
    pub path: String,
    // TODO: Runtime enforcement for request timeout.
    #[serde(default, with = "humantime_serde")]
    pub timeout: Option<std::time::Duration>,
    // TODO: Runtime enforcement for retry policy.
    pub retry: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub destinations: Vec<WeightedDestination>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RetryPolicy {
    pub attempts: usize,
    pub retry_on: Vec<serde_json::Value>,
    #[serde(with = "humantime_serde")]
    pub per_try_timeout: std::time::Duration,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct HeaderOperations {
    pub add: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}
