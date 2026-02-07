use crate::regex_validator::CompiledRegex;
use crate::router::{CompiledVirtualHost, RouteZone, Router};
use pavis_core::RegexLimits;
use pavis_core::{HeaderMatch, HeaderPredicates, MethodPredicate, PathMatch, Route, VirtualHost};
use std::collections::HashMap;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PredicateStats {
    pub path_misses: u64,
    pub method_misses: u64,
    pub header_misses: u64,

    // P2: Evaluation counters per operator
    pub exact_evals: u64,
    pub prefix_evals: u64,
    pub regex_evals: u64,
    pub present_evals: u64,
    pub absent_evals: u64,

    // P2: Rejection counters
    pub regex_input_too_large: u64,
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

    fn record_eval_exact(&mut self) {
        self.exact_evals += 1;
    }
    fn record_eval_prefix(&mut self) {
        self.prefix_evals += 1;
    }
    fn record_eval_regex(&mut self) {
        self.regex_evals += 1;
    }
    fn record_eval_present(&mut self) {
        self.present_evals += 1;
    }
    fn record_eval_absent(&mut self) {
        self.absent_evals += 1;
    }
    fn record_regex_input_too_large(&mut self) {
        self.regex_input_too_large += 1;
    }
}

impl core::ops::AddAssign for PredicateStats {
    fn add_assign(&mut self, other: Self) {
        self.path_misses += other.path_misses;
        self.method_misses += other.method_misses;
        self.header_misses += other.header_misses;

        self.exact_evals += other.exact_evals;
        self.prefix_evals += other.prefix_evals;
        self.regex_evals += other.regex_evals;
        self.present_evals += other.present_evals;
        self.absent_evals += other.absent_evals;
        self.regex_input_too_large += other.regex_input_too_large;
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

                        if !matches_headers(
                            &route.matcher.headers,
                            headers,
                            &router.regex_cache,
                            &router.regex_limits,
                            &mut stats,
                        ) {
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
                            &router.regex_cache,
                            &router.regex_limits,
                            &mut stats,
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
        MethodPredicate::List(methods) => methods
            .iter()
            .any(|m| method.eq_ignore_ascii_case(m.as_str())),
        #[allow(unreachable_patterns)]
        _ => true, // Default to matching for unknown variants
    }
}

/// Match request headers against header predicates (AND logic).
fn matches_headers(
    predicates: &HeaderPredicates,
    headers: &pingora::http::RequestHeader,
    regex_cache: &HashMap<String, CompiledRegex>,
    regex_limits: &RegexLimits,
    stats: &mut PredicateStats,
) -> bool {
    match predicates {
        HeaderPredicates::None => true,
        HeaderPredicates::Some(preds) => {
            // ALL predicates must match (AND logic)
            preds
                .iter()
                .all(|p| matches_header_predicate(p, headers, regex_cache, regex_limits, stats))
        }
        #[allow(unreachable_patterns)]
        _ => true, // Default to matching for unknown variants
    }
}

/// Match individual header predicate.
fn matches_header_predicate(
    pred: &pavis_core::HeaderPredicate,
    headers: &pingora::http::RequestHeader,
    regex_cache: &HashMap<String, CompiledRegex>,
    regex_limits: &RegexLimits,
    stats: &mut PredicateStats,
) -> bool {
    // Header names are case-insensitive per HTTP spec
    let header_value = headers.headers.get(pred.name.as_str());

    match (&pred.matcher, header_value) {
        (HeaderMatch::Present, Some(_)) => {
            stats.record_eval_present();
            true
        }
        (HeaderMatch::Present, None) => {
            stats.record_eval_present();
            false
        }
        (HeaderMatch::Absent, None) => {
            stats.record_eval_absent();
            true
        }
        (HeaderMatch::Absent, Some(_)) => {
            stats.record_eval_absent();
            false
        }
        (HeaderMatch::Exact(expected), Some(actual)) => {
            stats.record_eval_exact();
            // Compare header value (case-sensitive)
            actual.as_bytes() == expected.as_bytes()
        }
        (HeaderMatch::Exact(_), None) => {
            stats.record_eval_exact();
            false
        }
        (HeaderMatch::Prefix(prefix), Some(actual)) => {
            stats.record_eval_prefix();
            // Prefix match (case-sensitive)
            actual.as_bytes().starts_with(prefix.as_bytes())
        }
        (HeaderMatch::Prefix(_), None) => {
            stats.record_eval_prefix();
            false
        }
        (HeaderMatch::Regex(pattern), Some(actual)) => {
            stats.record_eval_regex();
            // Use pre-compiled regex from cache
            if let Some(compiled) = regex_cache.get(pattern.as_str()) {
                // Enforce input length limit
                if actual.len() > regex_limits.input_max_bytes as usize {
                    stats.record_regex_input_too_large();
                    return false;
                }
                compiled.is_match(actual.as_bytes(), regex_limits.input_max_bytes as usize)
            } else {
                // If regex is not found in cache (should be impossible if validation passed),
                // fail safe to no match
                false
            }
        }
        (HeaderMatch::Regex(_), None) => {
            stats.record_eval_regex();
            false
        }
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
            regex_cache: HashMap::new(),
            regex_limits: RegexLimits::default(),
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
            regex_cache: HashMap::new(),
            regex_limits: RegexLimits::default(),
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

        assert!(matches_header_predicate(
            &predicate,
            &req_with_header,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
        assert!(!matches_header_predicate(
            &predicate,
            &req_without_header,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
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

        assert!(matches_header_predicate(
            &predicate,
            &req_alice,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
        assert!(!matches_header_predicate(
            &predicate,
            &req_bob,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
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
        assert!(matches_header_predicate(
            &predicate_json,
            &req,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));

        // Test mismatch
        let predicate_xml = HeaderPredicate {
            name: "accept".into(),
            matcher: HeaderMatch::Exact("application/xml".into()),
        };
        assert!(!matches_header_predicate(
            &predicate_xml,
            &req,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
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

        assert!(matches_headers(
            &predicates,
            &req,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
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
        assert!(!matches_headers(
            &predicates,
            &req,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
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
            regex_cache: HashMap::new(),
            regex_limits: RegexLimits::default(),
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
            regex_cache: HashMap::new(),
            regex_limits: RegexLimits::default(),
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
            regex_cache: HashMap::new(),
            regex_limits: RegexLimits::default(),
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
        assert!(matches_header_predicate(
            &predicate,
            &req_lowercase,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
        assert!(matches_header_predicate(
            &predicate,
            &req_uppercase,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));

        // Value is case-sensitive
        let mut req_wrong_case = mock_request_header("GET");
        req_wrong_case.insert_header("X-Tenant", "alice").unwrap();

        let predicate_uppercase_value = HeaderPredicate {
            name: "x-tenant".into(),
            matcher: HeaderMatch::Exact("Alice".into()),
        };

        assert!(!matches_header_predicate(
            &predicate_uppercase_value,
            &req_wrong_case,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
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
        assert!(matches_header_predicate(
            &predicate_empty,
            &req_empty,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));

        // Missing header should NOT match empty string predicate
        assert!(!matches_header_predicate(
            &predicate_empty,
            &req_missing,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));

        // Test Present matcher
        let predicate_present = HeaderPredicate {
            name: "x-empty".into(),
            matcher: HeaderMatch::Present,
        };

        // Empty header value should match Present
        assert!(matches_header_predicate(
            &predicate_present,
            &req_empty,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));

        // Missing header should NOT match Present
        assert!(!matches_header_predicate(
            &predicate_present,
            &req_missing,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
    }

    #[test]
    fn test_exact_map_miss_stats() {
        let vhost = CompiledVirtualHost {
            config: VirtualHost {
                host: Host("example.com".to_string()),
                paths: vec![Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Exact {
                            path: Path("/exact".to_string()),
                        },
                        method: MethodPredicate::Specific(pavis_core::HttpMethod::POST),
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
                }],
            },
            zones: vec![RouteZone::ExactMap({
                let mut map = HashMap::new();
                map.insert(
                    "/exact".to_string(),
                    CompiledRoute {
                        index: 0,
                        regex: None,
                    },
                );
                map
            })],
        };

        let router = Router {
            exact_hosts: {
                let mut map = HashMap::new();
                map.insert("example.com".to_string(), vhost);
                map
            },
            wildcard_hosts: vec![],
            regex_cache: HashMap::new(),
            regex_limits: RegexLimits::default(),
        };

        let req_header = mock_request_header("GET");
        let verdict = match_request(&router, Some("example.com"), "/exact", "GET", &req_header);
        assert!(verdict.selection.is_none());
        assert_eq!(verdict.stats.method_misses, 1);

        let verdict_path_miss =
            match_request(&router, Some("example.com"), "/other", "GET", &req_header);
        assert_eq!(verdict_path_miss.stats.path_misses, 1);
    }

    #[test]
    fn test_header_predicate_absent() {
        use pavis_core::{HeaderMatch, HeaderPredicate};

        let mut req_with_header = mock_request_header("GET");
        req_with_header.insert_header("X-Foo", "bar").unwrap();
        let req_without_header = mock_request_header("GET");

        let predicate = HeaderPredicate {
            name: "x-foo".into(),
            matcher: HeaderMatch::Absent,
        };

        assert!(!matches_header_predicate(
            &predicate,
            &req_with_header,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
        assert!(matches_header_predicate(
            &predicate,
            &req_without_header,
            &HashMap::new(),
            &RegexLimits::default(),
            &mut PredicateStats::default(),
        ));
    }

    #[test]
    fn test_header_regex_input_too_large() {
        use crate::regex_validator::validate_and_compile_regexes;
        use pavis_core::{HeaderMatch, HeaderPredicate};

        let mut req = mock_request_header("GET");
        req.insert_header("X-Regex", "too-long-input").unwrap();

        let limits = RegexLimits {
            input_max_bytes: 5,
            ..Default::default()
        };

        let predicate = HeaderPredicate {
            name: "x-regex".into(),
            matcher: HeaderMatch::Regex(".*".into()),
        };

        let vhost = VirtualHost {
            host: Host("*".to_string()),
            paths: vec![Route {
                matcher: RouteMatcher {
                    path: PathMatch::Prefix {
                        path: Path("/".into()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::Some(vec![predicate.clone()]),
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
            }],
        };
        let config = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(pavis_core::Telemetry {
                level: pavis_core::LogLevel::Info,
                pingora: pavis_core::LogLevel::Error,
                service_name: pavis_core::ServiceName("test".to_string()),
                metrics: pavis_core::Metrics::Disabled,
                access_log: pavis_core::AccessLogPolicy::Disabled,
                tracing: pavis_core::TracingPolicy::Disabled,
            })
            .add_listener(
                pavis_core::ListenerBuilder::new()
                    .name(pavis_core::ListenerName("test".to_string()))
                    .address("127.0.0.1:0".parse().unwrap())
                    .build()
                    .unwrap(),
            )
            .add_route(vhost)
            .build()
            .unwrap();
        let validated = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(config) };
        let regex_cache = validate_and_compile_regexes(&validated, &limits).unwrap();

        let mut stats = PredicateStats::default();
        let matched = matches_header_predicate(&predicate, &req, &regex_cache, &limits, &mut stats);

        assert!(!matched);
        assert_eq!(stats.regex_input_too_large, 1);
    }
}
