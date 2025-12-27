//! Router module: Request matching and routing logic.
//!
//! # Architectural Invariants
//!
//! 1. **Deterministic Matching**: Route matching order is significant and must be deterministic.
//! 2. **Pre-compiled Regex**: All regular expressions must be compiled at initialization time, never during request handling.
//! 3. **Read-Only**: The router state is immutable after initialization.

use anyhow::{Context, Result};
use pavis_core::config::{MatchType, Route, VirtualHost};
use regex::Regex;

pub mod matcher;

pub struct Router {
    routes: Vec<VirtualHost>,
}

impl Router {
    pub fn new(routes: &[VirtualHost]) -> Result<Self> {
        let mut compiled_routes = routes.to_vec();
        for vhost in &mut compiled_routes {
            for route in &mut vhost.paths {
                if route.match_type == MatchType::Regex {
                    route.compiled_regex = Some(Regex::new(&route.path).with_context(|| {
                        format!("Failed to compile regex for path: {}", route.path)
                    })?);
                }
            }
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
    use pavis_core::config::{MatchType, Route, VirtualHost};

    #[test]
    fn test_invalid_regex_compilation() {
        let routes = vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Regex,
                path: "[unclosed".to_string(),
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![],
                compiled_regex: None,
            }],
        }];

        assert!(Router::new(&routes).is_err());
    }
}
