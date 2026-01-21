use crate::runtime::HeadersPolicy;
use crate::runtime::types::{Host, Hostname, Path, Timeout, TryTimeout, UpstreamName, Weight};
use compact_str::CompactString;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::num::NonZeroU16;
use std::sync::Arc;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct VirtualHost {
    pub host: Host,
    pub paths: Vec<Route>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Route {
    pub matcher: RouteMatcher,
    pub timeout: Timeout,
    pub retry: RetryPolicy,
    pub request_headers: Arc<HeadersPolicy>,
    pub response_headers: Arc<HeadersPolicy>,
    pub rewrite: Rewrite,
    pub action: RouteAction,
    pub principal: Principal,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum Principal {
    Any,
    Authenticated { spiffe: String },
    Prefix { prefix: String },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum RouteAction {
    Forward(Vec<Destination>),
    Redirect { status: u16, location: String },
    Direct { status: u16, body: String },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Rewrite {
    pub path: RewritePath,
    pub host: RewriteHost,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum RetryPolicy {
    Disabled,
    Enabled {
        attempts: NonZeroU16,
        per_try: TryTimeout,
        on: RetryFlags,
    },
}

/// Composite route matcher supporting path, method, and header predicates.
///
/// Evaluation order: path → method → headers (short-circuit on first mismatch).
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct RouteMatcher {
    pub path: PathMatch,
    pub method: MethodPredicate,
    pub headers: HeaderPredicates,
}

#[repr(u8)]
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum PathMatch {
    Prefix { path: Path },
    Exact { path: Path },
    Regex { path: Path },
}

/// HTTP method matching predicate.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum MethodPredicate {
    /// Match any HTTP method.
    Any,
    /// Match a specific HTTP method (case-insensitive per RFC 7231).
    Specific(HttpMethod),
}

/// Standard HTTP methods (RFC 7231 + CONNECT/TRACE).
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "UPPERCASE"))]
#[archive(check_bytes)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
    CONNECT,
    TRACE,
}

impl HttpMethod {
    /// Returns the uppercase string representation of the HTTP method.
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::GET => "GET",
            HttpMethod::POST => "POST",
            HttpMethod::PUT => "PUT",
            HttpMethod::DELETE => "DELETE",
            HttpMethod::PATCH => "PATCH",
            HttpMethod::HEAD => "HEAD",
            HttpMethod::OPTIONS => "OPTIONS",
            HttpMethod::CONNECT => "CONNECT",
            HttpMethod::TRACE => "TRACE",
        }
    }
}

/// Collection of header predicates (all must match for route to match).
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum HeaderPredicates {
    /// No header matching (route matches regardless of headers).
    None,
    /// Multiple header predicates with AND logic (all must match).
    Some(Vec<HeaderPredicate>),
}

/// Individual header matching predicate.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct HeaderPredicate {
    /// Header name (case-insensitive per HTTP spec).
    pub name: CompactString,
    /// Matching strategy for header value.
    pub matcher: HeaderMatch,
}

/// Header value matching strategies.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum HeaderMatch {
    /// Header must be present (any value accepted).
    Present,
    /// Header value must exactly match (case-sensitive).
    Exact(CompactString),
    /// Header value must match regex pattern.
    /// Pattern is stored as string; runtime compiles and caches regex.
    Regex(CompactString),
    /// Header must NOT be present.
    Absent,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct RetryFlags(pub u8);

#[allow(dead_code)]
pub const RETRY_FIVE_XX: u8 = 0b0000_0001;
#[allow(dead_code)]
pub const RETRY_CONNECT_FAILURE: u8 = 0b0000_0010;
#[allow(dead_code)]
pub const RETRY_RESET: u8 = 0b0000_0100;
#[allow(dead_code)]
pub const RETRY_REFUSED: u8 = 0b0000_1000;
#[allow(dead_code)]
pub const RETRY_RESERVED: u8 = 0b1111_0000;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Destination {
    pub upstream: UpstreamName,
    pub weight: Weight,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum RewritePath {
    Disabled,
    Prefix { from: Path, to: Path },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum RewriteHost {
    Disabled,
    Literal { host: Hostname },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_matcher_construction() {
        let matcher = RouteMatcher {
            path: PathMatch::Prefix {
                path: Path("/api".to_string()),
            },
            method: MethodPredicate::Any,
            headers: HeaderPredicates::None,
        };

        assert!(matches!(matcher.path, PathMatch::Prefix { .. }));
        assert_eq!(matcher.method, MethodPredicate::Any);
        assert!(matches!(matcher.headers, HeaderPredicates::None));
    }

    #[test]
    fn test_method_predicate_specific() {
        let predicate = MethodPredicate::Specific(HttpMethod::POST);
        assert!(matches!(
            predicate,
            MethodPredicate::Specific(HttpMethod::POST)
        ));
    }

    #[test]
    fn test_http_method_as_str() {
        assert_eq!(HttpMethod::GET.as_str(), "GET");
        assert_eq!(HttpMethod::POST.as_str(), "POST");
        assert_eq!(HttpMethod::PUT.as_str(), "PUT");
        assert_eq!(HttpMethod::DELETE.as_str(), "DELETE");
        assert_eq!(HttpMethod::PATCH.as_str(), "PATCH");
        assert_eq!(HttpMethod::HEAD.as_str(), "HEAD");
        assert_eq!(HttpMethod::OPTIONS.as_str(), "OPTIONS");
        assert_eq!(HttpMethod::CONNECT.as_str(), "CONNECT");
        assert_eq!(HttpMethod::TRACE.as_str(), "TRACE");
    }

    #[test]
    fn test_header_predicates_none() {
        let predicates = HeaderPredicates::None;
        assert!(matches!(predicates, HeaderPredicates::None));
    }

    #[test]
    fn test_header_predicates_some() {
        let predicates = HeaderPredicates::Some(vec![
            HeaderPredicate {
                name: CompactString::new("Authorization"),
                matcher: HeaderMatch::Present,
            },
            HeaderPredicate {
                name: CompactString::new("X-API-Version"),
                matcher: HeaderMatch::Exact(CompactString::new("v2")),
            },
        ]);

        if let HeaderPredicates::Some(preds) = predicates {
            assert_eq!(preds.len(), 2);
            assert_eq!(preds[0].name, "Authorization");
            assert!(matches!(preds[0].matcher, HeaderMatch::Present));
            assert_eq!(preds[1].name, "X-API-Version");
            if let HeaderMatch::Exact(val) = &preds[1].matcher {
                assert_eq!(val.as_str(), "v2");
            } else {
                panic!("Expected Exact match");
            }
        } else {
            panic!("Expected Some variant");
        }
    }

    #[test]
    fn test_header_match_variants() {
        let present = HeaderMatch::Present;
        let exact = HeaderMatch::Exact(CompactString::new("value"));
        let regex = HeaderMatch::Regex(CompactString::new("^v[0-9]+$"));
        let absent = HeaderMatch::Absent;

        assert!(matches!(present, HeaderMatch::Present));
        assert!(matches!(exact, HeaderMatch::Exact(_)));
        assert!(matches!(regex, HeaderMatch::Regex(_)));
        assert!(matches!(absent, HeaderMatch::Absent));
    }

    #[test]
    fn test_route_matcher_rkyv_serialization() {
        use rkyv::ser::{Serializer, serializers::AllocSerializer};

        let matcher = RouteMatcher {
            path: PathMatch::Exact {
                path: Path("/test".to_string()),
            },
            method: MethodPredicate::Specific(HttpMethod::GET),
            headers: HeaderPredicates::Some(vec![HeaderPredicate {
                name: CompactString::new("Content-Type"),
                matcher: HeaderMatch::Exact(CompactString::new("application/json")),
            }]),
        };

        let mut serializer = AllocSerializer::<256>::default();
        serializer.serialize_value(&matcher).unwrap();
        let bytes = serializer.into_serializer().into_inner();

        // Verify we can validate the archived bytes
        let result = rkyv::check_archived_root::<RouteMatcher>(&bytes);
        assert!(result.is_ok(), "rkyv validation failed");
    }

    #[test]
    fn test_complex_route_matcher() {
        let matcher = RouteMatcher {
            path: PathMatch::Regex {
                path: Path("/api/v[0-9]+/.*".to_string()),
            },
            method: MethodPredicate::Specific(HttpMethod::POST),
            headers: HeaderPredicates::Some(vec![
                HeaderPredicate {
                    name: CompactString::new("Authorization"),
                    matcher: HeaderMatch::Present,
                },
                HeaderPredicate {
                    name: CompactString::new("X-Tenant"),
                    matcher: HeaderMatch::Regex(CompactString::new("^tenant-[a-z]+$")),
                },
            ]),
        };

        assert!(matches!(matcher.path, PathMatch::Regex { .. }));
        assert_eq!(matcher.method, MethodPredicate::Specific(HttpMethod::POST));

        if let HeaderPredicates::Some(preds) = matcher.headers {
            assert_eq!(preds.len(), 2);
        } else {
            panic!("Expected Some variant");
        }
    }
}
