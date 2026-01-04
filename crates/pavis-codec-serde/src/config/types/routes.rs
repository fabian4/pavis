use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VirtualHost {
    pub host: String,
    pub paths: Vec<Route>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Route {
    #[serde(default)]
    pub matcher: Matcher,
    #[serde(default, with = "humantime_serde")]
    pub timeout: Option<std::time::Duration>,
    pub retry: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub rewrite: Option<RewritePolicy>,
    pub destinations: Vec<WeightedDestination>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Matcher {
    Prefix { path: String },
    Exact { path: String },
    Regex { path: String },
}

impl Default for Matcher {
    fn default() -> Self {
        Matcher::Prefix {
            path: "/".to_string(),
        }
    }
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
    #[serde(default)]
    pub set_headers: Vec<(String, String)>,
    #[serde(default)]
    pub append_headers: Vec<(String, String)>,
    #[serde(default)]
    pub add_headers: Vec<(String, String)>,
    #[serde(default)]
    pub remove_headers: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}
