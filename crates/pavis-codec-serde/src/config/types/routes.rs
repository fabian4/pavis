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
    /// List of HTTP methods (optional, P2 feature).
    pub methods: Option<Vec<String>>,
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

/// Header matching predicate supporting both Legacy (P0) and P2 DTOs.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum HeaderPredicate {
    V2(HeaderMatcherDTO),
    V1(HeaderPredicateLegacy),
}

/// Legacy P0 header matching predicate.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct HeaderPredicateLegacy {
    /// Header name (case-insensitive per HTTP spec).
    pub name: String,
    /// Header value to match (optional, defaults to presence check).
    pub value: Option<String>,
    /// If true, treat value as regex pattern (default: false).
    #[serde(default)]
    pub regex: bool,
    /// If true, use prefix matching instead of exact (default: false).
    #[serde(default)]
    pub prefix: bool,
    /// If true, header must NOT be present (default: false).
    #[serde(default)]
    pub absent: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RewritePolicy {
    pub path: Option<String>,
    pub host: Option<String>,
}

/// Retry policy DTO with full P2 features
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum attempts (including initial attempt)
    /// Valid range: 1..=10
    #[serde(default = "default_max_attempts", alias = "attempts")]
    pub max_attempts: u16,

    /// Retryable reasons (which failure types should trigger retries)
    #[serde(default = "default_retryable_reasons", alias = "retry_on")]
    pub retryable_reasons: Vec<String>,

    /// Retryable status codes (required when "status_code" is in retryable_reasons)
    #[serde(default)]
    pub retryable_status_codes: Option<Vec<u16>>,

    /// Backoff strategy
    #[serde(default)]
    pub backoff: BackoffStrategyDTO,

    /// Enable retries for non-idempotent methods (POST, PUT, PATCH, DELETE)
    #[serde(default)]
    pub retry_non_idempotent: bool,

    /// Fail with 500 if retry is required but body is not replayable
    #[serde(default)]
    pub fail_on_non_replayable_retry: bool,

    /// Maximum request body size to buffer in memory for replay (bytes)
    #[serde(default = "default_max_body_buffer")]
    pub max_request_body_buffer_bytes: u64,

    /// Per-try timeout
    #[serde(default, with = "humantime_serde")]
    pub per_try: Option<std::time::Duration>,
}

fn default_max_attempts() -> u16 {
    1
}

fn default_retryable_reasons() -> Vec<String> {
    vec![
        "status_code".to_string(),
        "connect_timeout".to_string(),
        "read_timeout".to_string(),
    ]
}

fn default_max_body_buffer() -> u64 {
    1_048_576 // 1MB
}

/// Backoff strategy DTO
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(tag = "strategy", rename_all = "lowercase")]
pub enum BackoffStrategyDTO {
    Fixed {
        #[serde(default = "default_backoff_base_ms")]
        base_ms: u64,
    },
    Linear {
        #[serde(default = "default_backoff_base_ms")]
        base_ms: u64,
    },
    Exponential {
        #[serde(default = "default_backoff_base_ms")]
        base_ms: u64,
        #[serde(default = "default_backoff_max_ms")]
        max_ms: u64,
    },
}

fn default_backoff_base_ms() -> u64 {
    100
}

fn default_backoff_max_ms() -> u64 {
    5000
}

impl Default for BackoffStrategyDTO {
    fn default() -> Self {
        Self::Exponential {
            base_ms: 100,
            max_ms: 5000,
        }
    }
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

/// Predicate AST DTO (P2 explicit DSL)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PredicateNodeDTO {
    #[serde(rename = "true")]
    True,

    #[serde(rename = "false")]
    False,

    Method {
        method: String,
    },

    Methods {
        methods: Vec<String>,
    },

    Header {
        #[serde(flatten)]
        matcher: HeaderMatcherDTO,
    },

    And {
        predicates: Vec<PredicateNodeDTO>,
    },

    Or {
        predicates: Vec<PredicateNodeDTO>,
    },

    Not {
        predicate: Box<PredicateNodeDTO>,
    },
}

/// Header matcher DTO for P2 explicit predicates
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "operator", rename_all = "lowercase")]
pub enum HeaderMatcherDTO {
    Exact { name: String, value: String },
    Prefix { name: String, prefix: String },
    Regex { name: String, pattern: String },
    Present { name: String },
    Absent { name: String },
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct WeightedDestination {
    pub upstream: String,
    pub weight: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_defaults() {
        let policy: RetryPolicy = serde_json::from_str("{}").unwrap();
        assert_eq!(policy.max_attempts, 1);
        assert_eq!(policy.retryable_reasons.len(), 3);
        assert_eq!(policy.max_request_body_buffer_bytes, 1_048_576);
        match policy.backoff {
            BackoffStrategyDTO::Exponential { base_ms, max_ms } => {
                assert_eq!(base_ms, 100);
                assert_eq!(max_ms, 5000);
            }
            _ => panic!("Expected exponential backoff"),
        }
    }

    #[test]
    fn test_backoff_strategy_dto_default() {
        let default = BackoffStrategyDTO::default();
        match default {
            BackoffStrategyDTO::Exponential { base_ms, max_ms } => {
                assert_eq!(base_ms, 100);
                assert_eq!(max_ms, 5000);
            }
            _ => panic!("Expected exponential backoff"),
        }
    }

    #[test]
    fn test_route_action_deserialization() {
        let forward: RouteAction =
            serde_json::from_str(r#"{"destinations": [{"upstream": "u1", "weight": 1}]}"#).unwrap();
        match forward {
            RouteAction::Forward { destinations } => {
                assert_eq!(destinations[0].upstream, "u1");
            }
            _ => panic!("Expected Forward"),
        }

        let redirect: RouteAction =
            serde_json::from_str(r#"{"status": 301, "location": "loc"}"#).unwrap();
        match redirect {
            RouteAction::Redirect { status, location } => {
                assert_eq!(status, 301);
                assert_eq!(location, "loc");
            }
            _ => panic!("Expected Redirect"),
        }

        let direct: RouteAction = serde_json::from_str(r#"{"status": 200, "body": "ok"}"#).unwrap();
        match direct {
            RouteAction::Direct { status, body } => {
                assert_eq!(status, 200);
                assert_eq!(body, "ok");
            }
            _ => panic!("Expected Direct"),
        }
    }

    #[test]
    fn test_principal_config_deserialization() {
        let any: PrincipalConfig = serde_json::from_str("\"any\"").unwrap();
        assert_eq!(any, PrincipalConfig::Any);

        let auth: PrincipalConfig =
            serde_json::from_str(r#"{"authenticated": {"spiffe": "s1"}}"#).unwrap();
        match auth {
            PrincipalConfig::Authenticated { spiffe } => assert_eq!(spiffe, "s1"),
            _ => panic!("Expected Authenticated"),
        }
    }

    #[test]
    fn test_backoff_strategy_field_defaults() {
        let s: BackoffStrategyDTO = serde_json::from_str(r#"{"strategy": "fixed"}"#).unwrap();
        match s {
            BackoffStrategyDTO::Fixed { base_ms } => assert_eq!(base_ms, 100),
            _ => panic!("Expected fixed"),
        }

        let s: BackoffStrategyDTO = serde_json::from_str(r#"{"strategy": "linear"}"#).unwrap();
        match s {
            BackoffStrategyDTO::Linear { base_ms } => assert_eq!(base_ms, 100),
            _ => panic!("Expected linear"),
        }

        let s: BackoffStrategyDTO = serde_json::from_str(r#"{"strategy": "exponential"}"#).unwrap();
        match s {
            BackoffStrategyDTO::Exponential { base_ms, max_ms } => {
                assert_eq!(base_ms, 100);
                assert_eq!(max_ms, 5000);
            }
            _ => panic!("Expected exponential"),
        }
    }

    #[test]
    fn test_predicate_node_dto_deserialization() {
        let t: PredicateNodeDTO = serde_json::from_str(r#"{"type": "true"}"#).unwrap();
        assert_eq!(t, PredicateNodeDTO::True);

        let f: PredicateNodeDTO = serde_json::from_str(r#"{"type": "false"}"#).unwrap();
        assert_eq!(f, PredicateNodeDTO::False);

        let m: PredicateNodeDTO =
            serde_json::from_str(r#"{"type": "method", "method": "GET"}"#).unwrap();
        match m {
            PredicateNodeDTO::Method { method } => assert_eq!(method, "GET"),
            _ => panic!("Expected Method"),
        }

        let h: PredicateNodeDTO =
            serde_json::from_str(r#"{"type": "header", "operator": "present", "name": "X-Foo"}"#)
                .unwrap();
        match h {
            PredicateNodeDTO::Header { matcher } => match matcher {
                HeaderMatcherDTO::Present { name } => assert_eq!(name, "X-Foo"),
                _ => panic!("Expected Present"),
            },
            _ => panic!("Expected Header"),
        }

        let and: PredicateNodeDTO =
            serde_json::from_str(r#"{"type": "and", "predicates": [{"type": "true"}]}"#).unwrap();
        match and {
            PredicateNodeDTO::And { predicates } => assert_eq!(predicates.len(), 1),
            _ => panic!("Expected And"),
        }
    }

    #[test]
    fn test_header_matcher_dto_variants() {
        let exact: HeaderMatcherDTO =
            serde_json::from_str(r#"{"operator": "exact", "name": "n", "value": "v"}"#).unwrap();
        assert!(matches!(exact, HeaderMatcherDTO::Exact { .. }));

        let prefix: HeaderMatcherDTO =
            serde_json::from_str(r#"{"operator": "prefix", "name": "n", "prefix": "p"}"#).unwrap();
        assert!(matches!(prefix, HeaderMatcherDTO::Prefix { .. }));

        let regex: HeaderMatcherDTO =
            serde_json::from_str(r#"{"operator": "regex", "name": "n", "pattern": "p"}"#).unwrap();
        assert!(matches!(regex, HeaderMatcherDTO::Regex { .. }));

        let absent: HeaderMatcherDTO =
            serde_json::from_str(r#"{"operator": "absent", "name": "n"}"#).unwrap();
        assert!(matches!(absent, HeaderMatcherDTO::Absent { .. }));
    }
}
