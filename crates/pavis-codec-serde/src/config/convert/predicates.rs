//! Predicate AST desugaring from DTO to core types
//!
//! This module handles conversion of P2 explicit predicate DSL and P0 sugar
//! into normalized PredicateNode AST for core validation and runtime use.

use anyhow::{Result, anyhow};
use compact_str::CompactString;
use pavis_core::HttpMethod;
use pavis_core::RegexLimits;
use pavis_core::matcher::{HeaderMatcher, MethodMatcher, PredicateNode};

use crate::config::types::{HeaderMatcherDTO, HeaderPredicate, PredicateNodeDTO};

impl PredicateNodeDTO {
    /// Convert DTO to core PredicateNode
    ///
    /// # Returns
    /// - `Ok((predicate, uses_advanced))` where `uses_advanced` indicates if P2 features were used
    ///
    /// # Layering Contract
    /// - Performs ONLY static validation (non-empty strings, byte length limits)
    /// - NO regex syntax validation or compilation (runtime responsibility)
    /// - Normalizes AST before returning
    pub fn to_core(&self, route_idx: usize, limits: &RegexLimits) -> Result<(PredicateNode, bool)> {
        let (node, uses_advanced) = self.to_core_recursive(route_idx, limits)?;
        Ok((node.normalize(), uses_advanced))
    }

    fn to_core_recursive(
        &self,
        route_idx: usize,
        limits: &RegexLimits,
    ) -> Result<(PredicateNode, bool)> {
        match self {
            PredicateNodeDTO::True => Ok((PredicateNode::True, false)),
            PredicateNodeDTO::False => Ok((PredicateNode::False, false)),

            PredicateNodeDTO::Method { method } => {
                let http_method = parse_http_method(method)?;
                Ok((
                    PredicateNode::Method(MethodMatcher::Exact(http_method)),
                    false,
                ))
            }

            PredicateNodeDTO::Methods { methods } => {
                if methods.is_empty() {
                    return Err(anyhow!(
                        "routes[{}].match.predicate: methods list cannot be empty",
                        route_idx
                    ));
                }
                let parsed: Result<Vec<HttpMethod>> =
                    methods.iter().map(|m| parse_http_method(m)).collect();
                let http_methods = parsed?;

                if http_methods.len() == 1 {
                    Ok((
                        PredicateNode::Method(MethodMatcher::Exact(http_methods[0])),
                        false,
                    ))
                } else {
                    Ok((
                        PredicateNode::Method(MethodMatcher::AnyOf(http_methods)),
                        false,
                    ))
                }
            }

            PredicateNodeDTO::Header { matcher } => matcher.to_core(route_idx, limits),

            PredicateNodeDTO::And { predicates } => {
                if predicates.is_empty() {
                    return Err(anyhow!(
                        "routes[{}].match.predicate: And predicates cannot be empty",
                        route_idx
                    ));
                }
                let mut uses_advanced = false;
                let children: Result<Vec<PredicateNode>> = predicates
                    .iter()
                    .map(|p| {
                        let (node, adv) = p.to_core_recursive(route_idx, limits)?;
                        uses_advanced |= adv;
                        Ok(node)
                    })
                    .collect();
                Ok((
                    PredicateNode::And(children?),
                    uses_advanced || predicates.len() > 1,
                ))
            }

            PredicateNodeDTO::Or { predicates } => {
                if predicates.is_empty() {
                    return Err(anyhow!(
                        "routes[{}].match.predicate: Or predicates cannot be empty",
                        route_idx
                    ));
                }
                let mut uses_advanced = false;
                let children: Result<Vec<PredicateNode>> = predicates
                    .iter()
                    .map(|p| {
                        let (node, adv) = p.to_core_recursive(route_idx, limits)?;
                        uses_advanced |= adv;
                        Ok(node)
                    })
                    .collect();
                // Or/Not are P2-only features
                Ok((PredicateNode::Or(children?), true))
            }

            PredicateNodeDTO::Not { predicate } => {
                let (node, _uses_advanced) = predicate.to_core_recursive(route_idx, limits)?;
                // Not is a P2-only feature
                Ok((PredicateNode::Not(Box::new(node)), true))
            }
        }
    }
}

impl HeaderMatcherDTO {
    /// Convert header matcher DTO to core type
    ///
    /// # Returns
    /// - `Ok((predicate, uses_advanced))` where `uses_advanced` indicates if P2 features were used
    pub fn to_core(&self, route_idx: usize, limits: &RegexLimits) -> Result<(PredicateNode, bool)> {
        let (header_matcher, uses_advanced) = match self {
            HeaderMatcherDTO::Exact { name, value } => {
                validate_header_name(name, route_idx)?;
                validate_header_value(value, route_idx)?;
                (
                    HeaderMatcher::Exact {
                        name: CompactString::new(name.to_ascii_lowercase()),
                        value: CompactString::new(value),
                    },
                    false, // Exact is P0
                )
            }

            HeaderMatcherDTO::Prefix { name, prefix } => {
                validate_header_name(name, route_idx)?;
                validate_header_value(prefix, route_idx)?;
                (
                    HeaderMatcher::Prefix {
                        name: CompactString::new(name.to_ascii_lowercase()),
                        prefix: CompactString::new(prefix),
                    },
                    true, // Prefix is P2
                )
            }

            HeaderMatcherDTO::Regex { name, pattern } => {
                validate_header_name(name, route_idx)?;
                // Static byte length check only (no compilation)
                if pattern.len() > limits.pattern_max_bytes as usize {
                    return Err(anyhow!(
                        "routes[{}].match.predicate: regex pattern for header '{}' exceeds {} bytes",
                        route_idx,
                        name,
                        limits.pattern_max_bytes
                    ));
                }
                (
                    HeaderMatcher::Regex {
                        name: CompactString::new(name.to_ascii_lowercase()),
                        pattern: CompactString::new(pattern),
                    },
                    true, // Regex is P2
                )
            }

            HeaderMatcherDTO::Present { name } => {
                validate_header_name(name, route_idx)?;
                (
                    HeaderMatcher::Present {
                        name: CompactString::new(name.to_ascii_lowercase()),
                    },
                    false, // Present is P0
                )
            }

            HeaderMatcherDTO::Absent { name } => {
                validate_header_name(name, route_idx)?;
                (
                    HeaderMatcher::Present {
                        name: CompactString::new(name.to_ascii_lowercase()),
                    },
                    true, // Absent is P2 (maps to Present for now, runtime will handle)
                )
            }
        };

        Ok((PredicateNode::Header(header_matcher), uses_advanced))
    }
}

/// Desugar P0 header predicates (sugar syntax) into PredicateNode
///
/// # Returns
/// - `Ok((predicate, uses_advanced))` where `uses_advanced` indicates if P2 features were used
pub fn desugar_p0_headers(
    headers: &[HeaderPredicate],
    route_idx: usize,
    limits: &RegexLimits,
) -> Result<(PredicateNode, bool)> {
    if headers.is_empty() {
        return Ok((PredicateNode::True, false));
    }

    let mut predicates = Vec::new();
    let mut uses_advanced = false;

    for (header_idx, pred_enum) in headers.iter().enumerate() {
        let (header_matcher, advanced) = match pred_enum {
            HeaderPredicate::V2(dto) => {
                let (node, adv) = dto.to_core(route_idx, limits)?;
                match node {
                    PredicateNode::Header(m) => (m, adv),
                    _ => {
                        return Err(anyhow!(
                            "routes[{}].match.headers[{}]: V2 DTO must be a header matcher",
                            route_idx,
                            header_idx
                        ));
                    }
                }
            }
            HeaderPredicate::V1(pred) => {
                let canonical_name = pred.name.to_ascii_lowercase();

                if pred.name.is_empty() {
                    return Err(anyhow!(
                        "routes[{}].match.headers[{}]: header name cannot be empty",
                        route_idx,
                        header_idx
                    ));
                }

                validate_header_name(&pred.name, route_idx)?;

                // Check for mutually exclusive flags
                let flag_count = [pred.regex, pred.prefix, pred.absent]
                    .iter()
                    .filter(|&&b| b)
                    .count();
                if flag_count > 1 {
                    return Err(anyhow!(
                        "routes[{}].match.headers[{}]: regex, prefix, and absent are mutually exclusive",
                        route_idx,
                        header_idx
                    ));
                }

                if pred.absent {
                    if pred.value.is_some() {
                        return Err(anyhow!(
                            "routes[{}].match.headers[{}]: absent=true cannot have a value",
                            route_idx,
                            header_idx
                        ));
                    }
                    (
                        HeaderMatcher::Present {
                            name: CompactString::new(&canonical_name),
                        },
                        true, // Absent is advanced
                    )
                } else if pred.regex {
                    let pattern = pred.value.as_ref().ok_or_else(|| {
                        anyhow!(
                            "routes[{}].match.headers[{}]: regex=true requires a value",
                            route_idx,
                            header_idx
                        )
                    })?;

                    if pattern.len() > limits.pattern_max_bytes as usize {
                        return Err(anyhow!(
                            "routes[{}].match.headers[{}]: regex pattern exceeds {} bytes",
                            route_idx,
                            header_idx,
                            limits.pattern_max_bytes
                        ));
                    }

                    (
                        HeaderMatcher::Regex {
                            name: CompactString::new(&canonical_name),
                            pattern: CompactString::new(pattern),
                        },
                        true,
                    )
                } else if pred.prefix {
                    let prefix_val = pred.value.as_ref().ok_or_else(|| {
                        anyhow!(
                            "routes[{}].match.headers[{}]: prefix=true requires a value",
                            route_idx,
                            header_idx
                        )
                    })?;

                    (
                        HeaderMatcher::Prefix {
                            name: CompactString::new(&canonical_name),
                            prefix: CompactString::new(prefix_val),
                        },
                        true,
                    )
                } else if let Some(value) = &pred.value {
                    validate_header_value(value, route_idx)?;
                    (
                        HeaderMatcher::Exact {
                            name: CompactString::new(&canonical_name),
                            value: CompactString::new(value),
                        },
                        false,
                    )
                } else {
                    // No value = presence check
                    (
                        HeaderMatcher::Present {
                            name: CompactString::new(&canonical_name),
                        },
                        false,
                    )
                }
            }
        };

        if advanced {
            uses_advanced = true;
        }
        predicates.push(PredicateNode::Header(header_matcher));
    }

    let result = if predicates.len() == 1 {
        predicates.into_iter().next().unwrap()
    } else {
        PredicateNode::And(predicates)
    };

    Ok((result, uses_advanced))
}

fn parse_http_method(method: &str) -> Result<HttpMethod> {
    match method.to_uppercase().as_str() {
        "GET" => Ok(HttpMethod::GET),
        "POST" => Ok(HttpMethod::POST),
        "PUT" => Ok(HttpMethod::PUT),
        "DELETE" => Ok(HttpMethod::DELETE),
        "PATCH" => Ok(HttpMethod::PATCH),
        "HEAD" => Ok(HttpMethod::HEAD),
        "OPTIONS" => Ok(HttpMethod::OPTIONS),
        "CONNECT" => Ok(HttpMethod::CONNECT),
        "TRACE" => Ok(HttpMethod::TRACE),
        _ => Err(anyhow!(
            "invalid HTTP method '{}' (supported: GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS, CONNECT, TRACE)",
            method
        )),
    }
}

fn validate_header_name(name: &str, route_idx: usize) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!(
            "routes[{}].match: header name cannot be empty",
            route_idx
        ));
    }

    // RFC 7230 token rules - basic check
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(anyhow!(
            "routes[{}].match: invalid header name '{}' (must contain only alphanumeric, -, or _)",
            route_idx,
            name
        ));
    }

    Ok(())
}

fn validate_header_value(value: &str, route_idx: usize) -> Result<()> {
    // RFC 7230: field-content = field-vchar [ 1*( SP / HTAB ) field-vchar ]
    // For simplicity, reject control characters except HTAB
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(anyhow!(
            "routes[{}].match: invalid header value (contains control characters)",
            route_idx
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_node_dto_true_false() {
        let limits = RegexLimits::default();

        let (node, adv) = PredicateNodeDTO::True.to_core(0, &limits).unwrap();
        assert_eq!(node, PredicateNode::True);
        assert!(!adv);

        let (node, adv) = PredicateNodeDTO::False.to_core(0, &limits).unwrap();
        assert_eq!(node, PredicateNode::False);
        assert!(!adv);
    }

    #[test]
    fn predicate_node_dto_method() {
        let limits = RegexLimits::default();

        let dto = PredicateNodeDTO::Method {
            method: "GET".to_string(),
        };
        let (node, adv) = dto.to_core(0, &limits).unwrap();
        assert!(matches!(
            node,
            PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET))
        ));
        assert!(!adv);
    }

    #[test]
    fn predicate_node_dto_methods() {
        let limits = RegexLimits::default();

        let dto = PredicateNodeDTO::Methods {
            methods: vec!["GET".to_string(), "POST".to_string()],
        };
        let (node, adv) = dto.to_core(0, &limits).unwrap();
        match node {
            PredicateNode::Method(MethodMatcher::AnyOf(methods)) => {
                assert_eq!(methods.len(), 2);
                assert!(methods.contains(&HttpMethod::GET));
                assert!(methods.contains(&HttpMethod::POST));
            }
            _ => panic!("expected AnyOf matcher"),
        }
        assert!(!adv);
    }

    #[test]
    fn predicate_node_dto_and() {
        let limits = RegexLimits::default();

        let dto = PredicateNodeDTO::And {
            predicates: vec![
                PredicateNodeDTO::Method {
                    method: "GET".to_string(),
                },
                PredicateNodeDTO::Header {
                    matcher: HeaderMatcherDTO::Present {
                        name: "x-foo".to_string(),
                    },
                },
            ],
        };
        let (node, adv) = dto.to_core(0, &limits).unwrap();
        assert!(matches!(node, PredicateNode::And(_)));
        assert!(adv); // Multiple predicates is advanced
    }

    #[test]
    fn predicate_node_dto_or_is_advanced() {
        let limits = RegexLimits::default();

        let dto = PredicateNodeDTO::Or {
            predicates: vec![
                PredicateNodeDTO::Method {
                    method: "GET".to_string(),
                },
                PredicateNodeDTO::Method {
                    method: "POST".to_string(),
                },
            ],
        };
        let (_, adv) = dto.to_core(0, &limits).unwrap();
        assert!(adv); // Or is P2 only
    }

    #[test]
    fn predicate_node_dto_not_is_advanced() {
        let limits = RegexLimits::default();

        let dto = PredicateNodeDTO::Not {
            predicate: Box::new(PredicateNodeDTO::Method {
                method: "GET".to_string(),
            }),
        };
        let (_, adv) = dto.to_core(0, &limits).unwrap();
        assert!(adv); // Not is P2 only
    }

    #[test]
    fn header_matcher_dto_exact() {
        let limits = RegexLimits::default();

        let dto = HeaderMatcherDTO::Exact {
            name: "X-Tenant".to_string(),
            value: "alice".to_string(),
        };
        let (node, adv) = dto.to_core(0, &limits).unwrap();
        match node {
            PredicateNode::Header(HeaderMatcher::Exact { name, value }) => {
                assert_eq!(name.as_str(), "x-tenant"); // lowercase
                assert_eq!(value.as_str(), "alice");
            }
            _ => panic!("expected exact matcher"),
        }
        assert!(!adv);
    }

    #[test]
    fn header_matcher_dto_prefix_is_advanced() {
        let limits = RegexLimits::default();

        let dto = HeaderMatcherDTO::Prefix {
            name: "x-tenant".to_string(),
            prefix: "team-".to_string(),
        };
        let (_, adv) = dto.to_core(0, &limits).unwrap();
        assert!(adv);
    }

    #[test]
    fn header_matcher_dto_regex_exceeds_limit() {
        let limits = RegexLimits {
            pattern_max_bytes: 10,
            ..Default::default()
        };

        let dto = HeaderMatcherDTO::Regex {
            name: "x-version".to_string(),
            pattern: "a".repeat(20),
        };
        let err = dto.to_core(0, &limits).unwrap_err();
        assert!(err.to_string().contains("exceeds 10 bytes"));
    }

    #[test]
    fn desugar_p0_headers_empty() {
        let limits = RegexLimits::default();
        let (node, adv) = desugar_p0_headers(&[], 0, &limits).unwrap();
        assert_eq!(node, PredicateNode::True);
        assert!(!adv);
    }

    #[test]
    fn desugar_p0_headers_exact() {
        let limits = RegexLimits::default();
        let headers = vec![HeaderPredicate::V1(
            crate::config::types::HeaderPredicateLegacy {
                name: "x-tenant".to_string(),
                value: Some("alice".to_string()),
                regex: false,
                prefix: false,
                absent: false,
            },
        )];
        let (node, adv) = desugar_p0_headers(&headers, 0, &limits).unwrap();
        assert!(matches!(
            node,
            PredicateNode::Header(HeaderMatcher::Exact { .. })
        ));
        assert!(!adv);
    }

    #[test]
    fn desugar_p0_headers_prefix_is_advanced() {
        let limits = RegexLimits::default();
        let headers = vec![HeaderPredicate::V1(
            crate::config::types::HeaderPredicateLegacy {
                name: "x-tenant".to_string(),
                value: Some("team-".to_string()),
                regex: false,
                prefix: true,
                absent: false,
            },
        )];
        let (_, adv) = desugar_p0_headers(&headers, 0, &limits).unwrap();
        assert!(adv);
    }

    #[test]
    fn desugar_p0_headers_mutually_exclusive() {
        let limits = RegexLimits::default();
        let headers = vec![HeaderPredicate::V1(
            crate::config::types::HeaderPredicateLegacy {
                name: "x-foo".to_string(),
                value: Some("bar".to_string()),
                regex: true,
                prefix: true,
                absent: false,
            },
        )];
        let err = desugar_p0_headers(&headers, 0, &limits).unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn parse_http_method_case_insensitive() {
        assert_eq!(parse_http_method("get").unwrap(), HttpMethod::GET);
        assert_eq!(parse_http_method("GET").unwrap(), HttpMethod::GET);
        assert_eq!(parse_http_method("Post").unwrap(), HttpMethod::POST);
    }

    #[test]
    fn parse_http_method_invalid() {
        let err = parse_http_method("INVALID").unwrap_err();
        assert!(err.to_string().contains("invalid HTTP method"));
    }

    #[test]
    fn validate_header_name_empty() {
        let err = validate_header_name("", 0).unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn validate_header_name_invalid_chars() {
        let err = validate_header_name("x foo", 0).unwrap_err();
        assert!(err.to_string().contains("invalid header name"));
    }

    #[test]
    fn validate_header_value_control_chars() {
        let err = validate_header_value("foo\x7fbar", 0).unwrap_err();
        assert!(err.to_string().contains("control characters"));

        // HTAB should be allowed
        assert!(validate_header_value("foo\tbar", 0).is_ok());
    }

    #[test]
    fn test_parse_http_method_all() {
        let methods = [
            "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "CONNECT", "TRACE",
        ];
        for m in methods {
            assert_eq!(parse_http_method(m).unwrap().as_str(), m);
        }
    }

    #[test]
    fn test_predicate_node_dto_methods_single() {
        let limits = RegexLimits::default();
        let dto = PredicateNodeDTO::Methods {
            methods: vec!["GET".to_string()],
        };
        let (node, adv) = dto.to_core(0, &limits).unwrap();
        assert!(matches!(
            node,
            PredicateNode::Method(MethodMatcher::Exact(HttpMethod::GET))
        ));
        assert!(!adv);
    }

    #[test]
    fn test_predicate_node_dto_empty_containers() {
        let limits = RegexLimits::default();

        let dto_and = PredicateNodeDTO::And { predicates: vec![] };
        assert!(dto_and.to_core(0, &limits).is_err());

        let dto_or = PredicateNodeDTO::Or { predicates: vec![] };
        assert!(dto_or.to_core(0, &limits).is_err());

        let dto_methods = PredicateNodeDTO::Methods { methods: vec![] };
        assert!(dto_methods.to_core(0, &limits).is_err());
    }

    #[test]
    fn test_header_matcher_dto_absent_and_regex() {
        let limits = RegexLimits::default();

        let dto_absent = HeaderMatcherDTO::Absent {
            name: "X-Foo".to_string(),
        };
        let (node, adv) = dto_absent.to_core(0, &limits).unwrap();
        assert!(matches!(
            node,
            PredicateNode::Header(HeaderMatcher::Present { .. })
        ));
        assert!(adv);

        let dto_regex = HeaderMatcherDTO::Regex {
            name: "X-Foo".to_string(),
            pattern: ".*".to_string(),
        };
        let (node, adv) = dto_regex.to_core(0, &limits).unwrap();
        assert!(matches!(
            node,
            PredicateNode::Header(HeaderMatcher::Regex { .. })
        ));
        assert!(adv);
    }

    #[test]
    fn test_desugar_p0_headers_v2_and_v1_failures() {
        let limits = RegexLimits::default();

        // V2 success
        let headers = vec![HeaderPredicate::V2(HeaderMatcherDTO::Present {
            name: "X-Foo".to_string(),
        })];
        let (node, adv) = desugar_p0_headers(&headers, 0, &limits).unwrap();
        assert!(matches!(
            node,
            PredicateNode::Header(HeaderMatcher::Present { .. })
        ));
        assert!(!adv);

        // V1 empty name
        let headers = vec![HeaderPredicate::V1(
            crate::config::types::HeaderPredicateLegacy {
                name: "".to_string(),
                value: None,
                regex: false,
                prefix: false,
                absent: false,
            },
        )];
        assert!(desugar_p0_headers(&headers, 0, &limits).is_err());

        // V1 absent with value
        let headers = vec![HeaderPredicate::V1(
            crate::config::types::HeaderPredicateLegacy {
                name: "X-Foo".to_string(),
                value: Some("val".to_string()),
                regex: false,
                prefix: false,
                absent: true,
            },
        )];
        assert!(desugar_p0_headers(&headers, 0, &limits).is_err());

        // V1 regex without value
        let headers = vec![HeaderPredicate::V1(
            crate::config::types::HeaderPredicateLegacy {
                name: "X-Foo".to_string(),
                value: None,
                regex: true,
                prefix: false,
                absent: false,
            },
        )];
        assert!(desugar_p0_headers(&headers, 0, &limits).is_err());

        // V1 prefix without value
        let headers = vec![HeaderPredicate::V1(
            crate::config::types::HeaderPredicateLegacy {
                name: "X-Foo".to_string(),
                value: None,
                regex: false,
                prefix: true,
                absent: false,
            },
        )];
        assert!(desugar_p0_headers(&headers, 0, &limits).is_err());
    }

    #[test]
    fn test_desugar_p0_headers_multiple_and_regex_limit() {
        let limits = RegexLimits {
            pattern_max_bytes: 5,
            ..Default::default()
        };

        let headers = vec![
            HeaderPredicate::V1(crate::config::types::HeaderPredicateLegacy {
                name: "X-A".to_string(),
                value: Some("a".to_string()),
                regex: false,
                prefix: false,
                absent: false,
            }),
            HeaderPredicate::V1(crate::config::types::HeaderPredicateLegacy {
                name: "X-B".to_string(),
                value: Some("b".to_string()),
                regex: false,
                prefix: false,
                absent: false,
            }),
        ];

        let (node, adv) = desugar_p0_headers(&headers, 0, &limits).unwrap();
        assert!(matches!(node, PredicateNode::And(_)));
        assert!(!adv);

        // Regex limit in P0
        let headers = vec![HeaderPredicate::V1(
            crate::config::types::HeaderPredicateLegacy {
                name: "X-Reg".to_string(),
                value: Some("123456".to_string()),
                regex: true,
                prefix: false,
                absent: false,
            },
        )];
        assert!(desugar_p0_headers(&headers, 0, &limits).is_err());
    }
}
