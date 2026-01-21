use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct VirtualHost {
    pub host: String,
    pub paths: Vec<Route>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct Route {
    pub matcher: Option<Matcher>,
    #[serde(default, with = "humantime_serde")]
    pub timeout: Option<std::time::Duration>,
    pub retry: Option<RetryPolicy>,
    pub request_headers: Option<HeaderOperations>,
    pub response_headers: Option<HeaderOperations>,
    pub rewrite: Option<RewritePolicy>,
    #[serde(flatten)]
    pub action: RouteAction,
    pub principal: Option<PrincipalConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PrincipalConfig {
    Any,
    Authenticated { spiffe: String },
    Prefix { prefix: String },
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(untagged, rename_all = "snake_case")]
pub enum RouteAction {
    Forward {
        destinations: Vec<WeightedDestination>,
    },
    Redirect {
        status: u16,
        location: String,
    },
    Direct {
        status: u16,
        body: String,
    },
}

/// Route matching configuration supporting path, method, and header predicates.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct Matcher {
    /// Path matching strategy (required).
    pub path: PathMatcher,
    /// HTTP method filter (optional, defaults to Any).
    pub method: Option<String>,
    /// Header predicates (optional, defaults to no header matching).
    pub headers: Option<Vec<HeaderPredicate>>,
}

/// Path matching strategy.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathMatcher {
    Prefix { path: String },
    Exact { path: String },
    Regex { path: String },
}

/// Header matching predicate.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct HeaderPredicate {
    /// Header name (case-insensitive per HTTP spec).
    pub name: String,
    /// Header value to match (optional, defaults to presence check).
    pub value: Option<String>,
    /// If true, treat value as regex pattern (default: false).
    #[serde(default)]
    pub regex: bool,
    /// If true, header must NOT be present (default: false).
    #[serde(default)]
    pub absent: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RewritePolicy {
    pub path: Option<String>,
    pub host: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub attempts: usize,
    pub retry_on: Vec<serde_json::Value>,
    #[serde(with = "humantime_serde")]
    pub per_try_timeout: std::time::Duration,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
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

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}
