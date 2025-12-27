//! Router module: Request matching and routing logic.
//!
//! # Architectural Invariants
//!
//! 1. **Deterministic Matching**: Route matching order is significant and must be deterministic.
//! 2. **Pre-compiled Regex**: All regular expressions must be compiled at initialization time, never during request handling.
//! 3. **Read-Only**: The router state is immutable after initialization.

use anyhow::{Context, Result};
use pavis_core::{MatchType, Route, VirtualHost};
use regex::Regex;

pub mod matcher;

pub struct CompiledVirtualHost {
    pub config: VirtualHost,
    pub regexes: Vec<Option<Regex>>,
}

pub struct Router {
    routes: Vec<CompiledVirtualHost>,
}

impl Router {
    pub fn new(routes: Vec<VirtualHost>) -> Result<Self> {
        let mut compiled_routes = Vec::new();
        for vhost in routes {
            let mut regexes = Vec::new();
            for route in &vhost.paths {
                let regex = if route.match_type == MatchType::Regex {
                    Some(Regex::new(&route.path).with_context(|| {
                        format!("Failed to compile regex for path: {}", route.path)
                    })?)
                } else {
                    None
                };
                regexes.push(regex);
            }
            compiled_routes.push(CompiledVirtualHost {
                config: vhost,
                regexes,
            });
        }
        Ok(Self {
            routes: compiled_routes,
        })
    }

    pub fn match_request<'a>(
        &'a self,
        host_header: Option<&str>,
        uri_path: &str,
    ) -> Option<(&'a VirtualHost, &'a Route)> {
        matcher::match_request(&self.routes, host_header, uri_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::{MatchType, Route, VirtualHost};

    #[test]
    fn test_invalid_regex_compilation() {
        let routes = vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Regex,
                path: "[unclosed".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![],
            }],
        }];

        assert!(Router::new(routes).is_err());
    }
}
