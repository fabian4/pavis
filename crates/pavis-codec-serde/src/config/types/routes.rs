use serde::{Deserialize, Serialize};

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
    #[serde(default, with = "humantime_serde")]
    pub timeout: Option<std::time::Duration>,
    pub retry: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub rewrite: Option<RewritePolicy>,
    pub destinations: Vec<WeightedDestination>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RewritePolicy {
    pub path_prefix_rewrite: Option<String>,
    pub host_rewrite_literal: Option<String>,
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
    pub actions: Vec<HeaderAction>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HeaderAction {
    pub key: String,
    pub value: Option<String>,
    pub action: pavis_core::HeaderActionType,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}
