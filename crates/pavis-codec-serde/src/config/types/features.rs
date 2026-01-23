use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RoutingFeatures {
    #[serde(default)]
    pub routing: RoutingFeatureConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RoutingFeatureConfig {
    #[serde(default)]
    pub advanced_matchers: bool,
    #[serde(default)]
    pub regex_limits: RegexLimits,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RegexLimits {
    pub pattern_max_bytes: Option<u32>,
    pub size_limit_bytes: Option<u64>,
    pub input_max_bytes: Option<u32>,
    pub max_regex_per_route: Option<u32>,
    pub max_regex_per_config: Option<u32>,
}
