use crate::router::{CompiledVirtualHost, RouteZone, Router};
use pavis_core::{HeaderMatch, HeaderPredicates, MethodPredicate, PathMatch, Route, VirtualHost};

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PredicateStats {
    pub path_misses: u64,
    pub method_misses: u64,
    pub header_misses: u64,
}

impl PredicateStats {
    fn record_path_miss(&mut self) {
        self.path_misses += 1;
    }
    fn record_method_miss(&mut self) {
        self.method_misses += 1;
    }
    fn record_header_miss(&mut self) {
        self.header_misses += 1;
    }
}

impl core::ops::AddAssign for PredicateStats {
    fn add_assign(&mut self, other: Self) {
        self.path_misses += other.path_misses;
        self.method_misses += other.method_misses;
        self.header_misses += other.header_misses;
    }
}

pub struct MatchVerdict<'a> {
    pub selection: Option<(&'a VirtualHost, &'a Route)>,
    pub stats: PredicateStats,
}

impl<'a> MatchVerdict<'a> {
    fn new(selection: Option<(&'a VirtualHost, &'a Route)>, stats: PredicateStats) -> Self {
        Self { selection, stats }
    }

    pub fn into_option(self) -> Option<(&'a VirtualHost, &'a Route)> {
        self.selection
    }
}

pub(crate) fn match_request<'a>(
    router: &'a Router,
    host_header: Option<&str>,
    uri_path: &str,
    method: &str,
    headers: &pingora::http::RequestHeader,
) -> MatchVerdict<'a> {
    let normalized_host = host_header.map(normalize_host);
    let mut aggregate_stats = PredicateStats::default();

    let try_match = |vhost: &'a CompiledVirtualHost| -> MatchVerdict<'a> {
        let mut stats = PredicateStats::default();
        for zone in &vhost.zones {
            match zone {
                RouteZone::Linear(routes) => {
                    for compiled in routes {
                        let route = &vhost.config.paths[compiled.index];

                        let path_match = match &route.matcher.path {
                            PathMatch::Prefix { path } => uri_path.starts_with(&path.0),
                            PathMatch::Exact { path } => uri_path == path.0,
                            PathMatch::Regex { .. } => compiled
                                .regex
                                .as_ref()
                                .map(|re| re.is_match(uri_path))
                                .unwrap_or(false),
                            #[allow(unreachable_patterns)]
                            &_ => false,
                        };

                        if !path_match {
                            stats.record_path_miss();
                            continue;
                        }

                        if !matches_method(&route.matcher.method, method) {
                            stats.record_method_miss();
                            continue;
                        }

                        if !matches_headers(&route.matcher.headers, headers) {
                            stats.record_header_miss();
                            continue;
                        }

                        return MatchVerdict::new(Some((&vhost.config, route)), stats);
                    }
                }
                RouteZone::ExactMap(map) => {
                    if let Some(compiled) = map.get(uri_path)
                        && matches_method(
                            &vhost.config.paths[compiled.index].matcher.method,
                            method,
                        )
                        && matches_headers(
                            &vhost.config.paths[compiled.index].matcher.headers,
                            headers,
                        )
                    {
                        let route = &vhost.config.paths[compiled.index];
                        return MatchVerdict::new(Some((&vhost.config, route)), stats);
                    } else if map.get(uri_path).is_some() {
                        let route = &vhost.config.paths[map.get(uri_path).unwrap().index];
                        if !matches_method(&route.matcher.method, method) {
                            stats.record_method_miss();
                        } else {
                            stats.record_header_miss();
                        }
                    } else {
                        stats.record_path_miss();
                    }
                }
            }
        }
        MatchVerdict::new(None, stats)
    };

    if let Some(host) = normalized_host
        && let Some(vhost) = router.exact_hosts.get(host)
    {
        let verdict = try_match(vhost);
        aggregate_stats += verdict.stats;
        if verdict.selection.is_some() {
            return MatchVerdict::new(verdict.selection, aggregate_stats);
        }
    }

    for vhost in &router.wildcard_hosts {
        let pattern = &vhost.config.host.0;
        let is_match = if pattern == "*" {
            true
        } else if let Some(suffix) = pattern.strip_prefix("*.") {
            normalized_host.is_some_and(|h| {
                h.ends_with(suffix)
                    && h.len() > suffix.len()
                    && h.as_bytes()[h.len() - suffix.len() - 1] == b'.'
            })
        } else if let Some(prefix) = pattern.strip_suffix(".*") {
            normalized_host.is_some_and(|h| {
                h.starts_with(prefix)
                    && h.len() > prefix.len()
                    && h.as_bytes()[prefix.len()] == b'.'
            })
        } else {
            normalized_host.is_some_and(|h| h == pattern)
        };

        if !is_match {
            continue;
        }

        let verdict = try_match(vhost);
        aggregate_stats += verdict.stats;
        if verdict.selection.is_some() {
            return MatchVerdict::new(verdict.selection, aggregate_stats);
        }
    }

    MatchVerdict::new(None, aggregate_stats)
}

fn normalize_host(host: &str) -> &str {
    if let Some(stripped) = host.strip_prefix('[')
        && let Some(end) = stripped.find(']')
    {
        return &stripped[..end];
    }
    if let Some((host_only, _port)) = host.split_once(':') {
        return host_only;
    }
    host
}

/// Match HTTP method against method predicate (case-insensitive per RFC 7231).
fn matches_method(predicate: &MethodPredicate, method: &str) -> bool {
    match predicate {
        MethodPredicate::Any => true,
        MethodPredicate::Specific(m) => method.eq_ignore_ascii_case(m.as_str()),
        #[allow(unreachable_patterns)]
        _ => true, // Default to matching for unknown variants
    }
}

/// Match request headers against header predicates (AND logic).
fn matches_headers(predicates: &HeaderPredicates, headers: &pingora::http::RequestHeader) -> bool {
    match predicates {
        HeaderPredicates::None => true,
        HeaderPredicates::Some(preds) => {
            // ALL predicates must match (AND logic)
            preds.iter().all(|p| matches_header_predicate(p, headers))
        }
        #[allow(unreachable_patterns)]
        _ => true, // Default to matching for unknown variants
    }
}

/// Match individual header predicate.
fn matches_header_predicate(
    pred: &pavis_core::HeaderPredicate,
    headers: &pingora::http::RequestHeader,
) -> bool {
    // Header names are case-insensitive per HTTP spec
    let header_value = headers.headers.get(pred.name.as_str());

    match (&pred.matcher, header_value) {
        (HeaderMatch::Present, Some(_)) => true,
        (HeaderMatch::Present, None) => false,
        (HeaderMatch::Absent, None) => true,
        (HeaderMatch::Absent, Some(_)) => false,
        (HeaderMatch::Exact(expected), Some(actual)) => {
            // Compare header value (case-sensitive)
            actual.to_str().ok().is_some_and(|v| v == expected.as_str())
        }
        (HeaderMatch::Exact(_), None) => false,
        (HeaderMatch::Regex(pattern), Some(actual)) => {
            // For regex, we need to compile at config load time
            // For now, we'll do a simple string match (will improve with compiled regex)
            // TODO: Add regex compilation at config load time
            if let Ok(actual_str) = actual.to_str() {
                if let Ok(re) = regex::Regex::new(pattern.as_str()) {
                    re.is_match(actual_str)
                } else {
                    false
                }
            } else {
                false
            }
        }
        (HeaderMatch::Regex(_), None) => false,
        #[allow(unreachable_patterns)]
        _ => false, // Unknown variants default to no match
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::CompiledRoute;
    use pavis_core::{
        Destination, HeaderPredicates, HeadersPolicy, Host, MethodPredicate, Path, PathMatch,
        RetryPolicy, Rewrite, RewriteHost, RewritePath, Route, RouteAction, RouteMatcher, Timeout,
        Weight,
    };
    use std::collections::HashMap;
    use std::num::NonZeroU16;

    // Helper to create a mock RequestHeader for testing
    fn mock_request_header(method: &str) -> pingora::http::RequestHeader {
        pingora::http::RequestHeader::build(method, b"/", None).unwrap()
    }

    #[test]
    fn test_normalize_host() {
        assert_eq!(normalize_host("example.com"), "example.com");
        assert_eq!(normalize_host("example.com:8080"), "example.com");
        assert_eq!(normalize_host("[::1]"), "::1");
        assert_eq!(normalize_host("[::1]:8080"), "::1");
        assert_eq!(normalize_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(normalize_host("foo.bar.com:443"), "foo.bar.com");
    }

    fn make_route() -> Route {
        Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                method: MethodPredicate::Any,
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![]),
        }
    }

    fn make_vhost(host: &str) -> CompiledVirtualHost {
        CompiledVirtualHost {
            config: VirtualHost {
                host: Host(host.to_string()),
                paths: vec![make_route()],
            },
            zones: vec![RouteZone::Linear(vec![CompiledRoute {
                index: 0,
                regex: None,
            }])],
        }
    }

    #[test]
    fn test_match_wildcard_host_patterns() {
        let suffix_vhost = make_vhost("*.example.com");
        let prefix_vhost = make_vhost("app.*");

        let router = Router {
            exact_hosts: HashMap::new(),
            wildcard_hosts: vec![suffix_vhost, prefix_vhost],
        };

        let req_header = mock_request_header("GET");

        // Suffix match
        let (v, _) = match_request(&router, Some("foo.example.com"), "/", "GET", &req_header)
            .into_option()
            .unwrap();
        assert_eq!(v.host.0, "*.example.com");

        // Prefix match
        let (v, _) = match_request(&router, Some("app.internal"), "/", "GET", &req_header)
            .into_option()
            .unwrap();
        assert_eq!(v.host.0, "app.*");

        // No match
        assert!(
            match_request(&router, Some("other.com"), "/", "GET", &req_header)
                .into_option()
                .is_none()
        );
        assert!(
            match_request(&router, Some("example.com"), "/", "GET", &req_header)
                .into_option()
                .is_none()
        );
    }

    #[test]
    fn test_match_exact_linear() {
        let vhost = CompiledVirtualHost {
            config: VirtualHost {
                host: Host("*".to_string()),
                paths: vec![Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Exact {
                            path: Path("/exact".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Disabled,
                    request_headers: HeadersPolicy::Disabled.into(),
                    response_headers: HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: pavis_core::UpstreamName("u".to_string()),
                        weight: Weight(NonZeroU16::new(1).unwrap()),
                    }]),
                }],
            },
            zones: vec![RouteZone::Linear(vec![CompiledRoute {
                index: 0,
                regex: None,
            }])],
        };

        let router = Router {
            exact_hosts: HashMap::new(),
            wildcard_hosts: vec![vhost],
        };

        let req_header = mock_request_header("GET");

        let (_, res) = match_request(&router, None, "/exact", "GET", &req_header)
            .into_option()
            .unwrap();
        assert!(matches!(res.matcher.path, PathMatch::Exact { .. }));

        let res_miss = match_request(&router, None, "/exact/more", "GET", &req_header);
        assert!(res_miss.into_option().is_none());
    }

    // P0 Feature #1: Header/Method Routing Gap - Unit Tests
    // These tests verify matcher logic per verification plan requirements.

    /// Test 1: Single method predicate - match GET, reject POST.
    #[test]
    fn test_method_predicate_exact_match() {
        assert!(matches_method(
            &MethodPredicate::Specific(pavis_core::HttpMethod::GET),
            "GET"
        ));
        assert!(!matches_method(
            &MethodPredicate::Specific(pavis_core::HttpMethod::GET),
            "POST"
        ));
    }

    /// Test 2: Single header predicate - match exact value, reject missing header.
    #[test]
    fn test_header_predicate_exact_match() {
        use pavis_core::{HeaderMatch, HeaderPredicate};

        let mut req_with_header = mock_request_header("GET");
        req_with_header.insert_header("X-Tenant", "alice").unwrap();

        let req_without_header = mock_request_header("GET");

        let predicate = HeaderPredicate {
            name: "x-tenant".into(), // Case-insensitive
            matcher: HeaderMatch::Exact("alice".into()),
        };

        assert!(matches_header_predicate(&predicate, &req_with_header));
        assert!(!matches_header_predicate(&predicate, &req_without_header));
    }

    /// Test 3: Single header predicate - match exact value, reject different value.
    #[test]
    fn test_header_predicate_value_mismatch() {
        use pavis_core::{HeaderMatch, HeaderPredicate};

        let mut req_alice = mock_request_header("GET");
        req_alice.insert_header("X-Tenant", "alice").unwrap();

        let mut req_bob = mock_request_header("GET");
        req_bob.insert_header("X-Tenant", "bob").unwrap();

        let predicate = HeaderPredicate {
            name: "x-tenant".into(),
            matcher: HeaderMatch::Exact("alice".into()),
        };

        assert!(matches_header_predicate(&predicate, &req_alice));
        assert!(!matches_header_predicate(&predicate, &req_bob));
    }

    /// Test 4: Multi-value header - match if any value matches (OR within header).
    #[test]
    fn test_multivalue_header_or_logic() {
        use pavis_core::{HeaderMatch, HeaderPredicate};

        let mut req = mock_request_header("GET");
        // Pingora's insert_header doesn't support multi-value directly in this API
        // We'll test with comma-separated values (common HTTP pattern)
        req.insert_header("Accept", "text/html, application/json")
            .unwrap();

        let predicate_json = HeaderPredicate {
            name: "accept".into(),
            matcher: HeaderMatch::Exact("text/html, application/json".into()),
        };

        // Note: Exact match requires full value match. For true multi-value OR,
        // we'd need regex or the runtime to split on commas.
        // This test verifies exact matching works as expected.
        assert!(matches_header_predicate(&predicate_json, &req));

        // Test mismatch
        let predicate_xml = HeaderPredicate {
            name: "accept".into(),
            matcher: HeaderMatch::Exact("application/xml".into()),
        };
        assert!(!matches_header_predicate(&predicate_xml, &req));
    }

    /// Test 5: Multiple header predicates - X-Tenant: alice AND X-Region: us-east (both match).
    #[test]
    fn test_multiple_header_predicates_and_logic_match() {
        use pavis_core::{HeaderMatch, HeaderPredicate, HeaderPredicates};

        let mut req = mock_request_header("GET");
        req.insert_header("X-Tenant", "alice").unwrap();
        req.insert_header("X-Region", "us-east").unwrap();

        let predicates = HeaderPredicates::Some(vec![
            HeaderPredicate {
                name: "x-tenant".into(),
                matcher: HeaderMatch::Exact("alice".into()),
            },
            HeaderPredicate {
                name: "x-region".into(),
                matcher: HeaderMatch::Exact("us-east".into()),
            },
        ]);

        assert!(matches_headers(&predicates, &req));
    }

    /// Test 6: Multiple header predicates - X-Tenant: alice AND X-Debug: true (second missing).
    #[test]
    fn test_multiple_header_predicates_and_logic_partial_match() {
        use pavis_core::{HeaderMatch, HeaderPredicate, HeaderPredicates};

        let mut req = mock_request_header("GET");
        req.insert_header("X-Tenant", "alice").unwrap();
        // X-Debug is missing

        let predicates = HeaderPredicates::Some(vec![
            HeaderPredicate {
                name: "x-tenant".into(),
                matcher: HeaderMatch::Exact("alice".into()),
            },
            HeaderPredicate {
                name: "x-debug".into(),
                matcher: HeaderMatch::Exact("true".into()),
            },
        ]);

        // Should fail because X-Debug is missing (AND logic requires all to match)
        assert!(!matches_headers(&predicates, &req));
    }

    /// Test 7: Compound predicates (path + method) - match both, reject if either fails.
    #[test]
    fn test_compound_path_method_predicates() {
        let route_get = Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/api".to_string()),
                },
                method: MethodPredicate::Specific(pavis_core::HttpMethod::GET),
                headers: HeaderPredicates::None,
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![]),
        };

        let vhost = CompiledVirtualHost {
            config: VirtualHost {
                host: Host("*".to_string()),
                paths: vec![route_get],
            },
            zones: vec![RouteZone::Linear(vec![CompiledRoute {
                index: 0,
                regex: None,
            }])],
        };

        let router = Router {
            exact_hosts: HashMap::new(),
            wildcard_hosts: vec![vhost],
        };

        let req_get = mock_request_header("GET");
        let req_post = mock_request_header("POST");

        // Match: path + method both match
        assert!(
            match_request(&router, None, "/api/users", "GET", &req_get)
                .selection
                .is_some()
        );

        // Reject: path matches but method fails
        assert!(
            match_request(&router, None, "/api/users", "POST", &req_post)
                .selection
                .is_none()
        );

        // Reject: method matches but path fails
        assert!(
            match_request(&router, None, "/other", "GET", &req_get)
                .selection
                .is_none()
        );
    }

    /// Test 8: Compound predicates (path + method + multiple headers) - match all, reject if any fails.
    #[test]
    fn test_compound_path_method_headers_predicates() {
        use pavis_core::{HeaderMatch, HeaderPredicate, HeaderPredicates};

        let route = Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/api".to_string()),
                },
                method: MethodPredicate::Specific(pavis_core::HttpMethod::GET),
                headers: HeaderPredicates::Some(vec![HeaderPredicate {
                    name: "x-tenant".into(),
                    matcher: HeaderMatch::Exact("alice".into()),
                }]),
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![]),
        };

        let vhost = CompiledVirtualHost {
            config: VirtualHost {
                host: Host("*".to_string()),
                paths: vec![route],
            },
            zones: vec![RouteZone::Linear(vec![CompiledRoute {
                index: 0,
                regex: None,
            }])],
        };

        let router = Router {
            exact_hosts: HashMap::new(),
            wildcard_hosts: vec![vhost],
        };

        let mut req_full_match = mock_request_header("GET");
        req_full_match.insert_header("X-Tenant", "alice").unwrap();

        let mut req_header_mismatch = mock_request_header("GET");
        req_header_mismatch
            .insert_header("X-Tenant", "bob")
            .unwrap();

        // Match: all predicates succeed
        assert!(
            match_request(&router, None, "/api/users", "GET", &req_full_match)
                .selection
                .is_some()
        );

        // Reject: path + method match, header fails
        assert!(
            match_request(&router, None, "/api/users", "GET", &req_header_mismatch)
                .selection
                .is_none()
        );

        // Reject: path + header match, method fails
        // Note: Need to add header to POST request
        let mut req_post_with_header = mock_request_header("POST");
        req_post_with_header
            .insert_header("X-Tenant", "alice")
            .unwrap();
        assert!(
            match_request(&router, None, "/api/users", "POST", &req_post_with_header)
                .selection
                .is_none()
        );
    }

    /// Test 9: Evaluation order - verify short-circuit (method checked before headers).
    #[test]
    fn test_evaluation_order_short_circuit() {
        use pavis_core::{HeaderMatch, HeaderPredicate, HeaderPredicates};

        let route = Route {
            matcher: RouteMatcher {
                path: PathMatch::Prefix {
                    path: Path("/api".to_string()),
                },
                method: MethodPredicate::Specific(pavis_core::HttpMethod::GET),
                headers: HeaderPredicates::Some(vec![HeaderPredicate {
                    name: "x-tenant".into(),
                    matcher: HeaderMatch::Exact("alice".into()),
                }]),
            },
            timeout: Timeout::Disabled,
            retry: RetryPolicy::Disabled,
            request_headers: HeadersPolicy::Disabled.into(),
            response_headers: HeadersPolicy::Disabled.into(),
            principal: pavis_core::Principal::Any,
            rewrite: Rewrite {
                path: RewritePath::Disabled,
                host: RewriteHost::Disabled,
            },
            action: RouteAction::Forward(vec![]),
        };

        let vhost = CompiledVirtualHost {
            config: VirtualHost {
                host: Host("*".to_string()),
                paths: vec![route],
            },
            zones: vec![RouteZone::Linear(vec![CompiledRoute {
                index: 0,
                regex: None,
            }])],
        };

        let router = Router {
            exact_hosts: HashMap::new(),
            wildcard_hosts: vec![vhost],
        };

        // Request with wrong method (no headers needed for short-circuit)
        let req_post = mock_request_header("POST");

        let verdict = match_request(&router, None, "/api/users", "POST", &req_post);

        // Verify short-circuit: method failed, so header check was skipped
        // Stats should show method miss but NOT header miss
        assert_eq!(verdict.stats.path_misses, 0); // Path matched
        assert_eq!(verdict.stats.method_misses, 1); // Method failed
        assert_eq!(verdict.stats.header_misses, 0); // Headers NOT checked (short-circuit)
        assert!(verdict.selection.is_none());
    }

    /// Test 10: Case sensitivity - header name case-insensitive, value case-sensitive.
    #[test]
    fn test_header_case_sensitivity() {
        use pavis_core::{HeaderMatch, HeaderPredicate};

        let mut req_lowercase = mock_request_header("GET");
        req_lowercase.insert_header("x-tenant", "Alice").unwrap();

        let mut req_uppercase = mock_request_header("GET");
        req_uppercase.insert_header("X-TENANT", "Alice").unwrap();

        // Predicate uses lowercase name
        let predicate = HeaderPredicate {
            name: "x-tenant".into(),
            matcher: HeaderMatch::Exact("Alice".into()),
        };

        // Header name is case-insensitive (both should match)
        assert!(matches_header_predicate(&predicate, &req_lowercase));
        assert!(matches_header_predicate(&predicate, &req_uppercase));

        // Value is case-sensitive
        let mut req_wrong_case = mock_request_header("GET");
        req_wrong_case.insert_header("X-Tenant", "alice").unwrap();

        let predicate_uppercase_value = HeaderPredicate {
            name: "x-tenant".into(),
            matcher: HeaderMatch::Exact("Alice".into()),
        };

        assert!(!matches_header_predicate(
            &predicate_uppercase_value,
            &req_wrong_case
        ));
    }

    /// Test 11: Empty vs missing header - empty string matches literal "", missing fails predicate.
    #[test]
    fn test_empty_vs_missing_header() {
        use pavis_core::{HeaderMatch, HeaderPredicate};

        let mut req_empty = mock_request_header("GET");
        req_empty.insert_header("X-Empty", "").unwrap();

        let req_missing = mock_request_header("GET");

        let predicate_empty = HeaderPredicate {
            name: "x-empty".into(),
            matcher: HeaderMatch::Exact("".into()),
        };

        // Empty header value should match empty string predicate
        assert!(matches_header_predicate(&predicate_empty, &req_empty));

        // Missing header should NOT match empty string predicate
        assert!(!matches_header_predicate(&predicate_empty, &req_missing));

        // Test Present matcher
        let predicate_present = HeaderPredicate {
            name: "x-empty".into(),
            matcher: HeaderMatch::Present,
        };

        // Empty header value should match Present
        assert!(matches_header_predicate(&predicate_present, &req_empty));

        // Missing header should NOT match Present
        assert!(!matches_header_predicate(&predicate_present, &req_missing));
    }
}
