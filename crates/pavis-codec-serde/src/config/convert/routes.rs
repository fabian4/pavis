use anyhow::Result;
use compact_str::CompactString;
use pavis_core::{ErrorCode, FieldPathBuilder, PavisError, RouteAction as CoreRouteAction};
use std::num::{NonZeroU16, NonZeroU32};

use crate::config::types::{
    HeaderMatcherDTO, HeaderOperations, HeaderPredicate, HeaderPredicateLegacy, Matcher,
    PathMatcher, PrincipalConfig, RetryPolicy, RewritePolicy, Route,
    RouteAction as CodecRouteAction, VirtualHost, WeightedDestination,
};

pub(super) fn to_runtime(routes: Vec<VirtualHost>) -> Result<Vec<pavis_core::VirtualHost>> {
    let mut runtime_routes = Vec::new();

    for (vh_index, v) in routes.into_iter().enumerate() {
        let mut paths = Vec::new();
        for (path_index, p) in v.paths.into_iter().enumerate() {
            let request_headers = to_runtime_headers(p.request_headers);
            let response_headers = to_runtime_headers(p.response_headers);
            let matcher_cfg = p.matcher.unwrap_or_else(default_matcher);

            let action = match p.action {
                CodecRouteAction::Forward { destinations } => {
                    let dests = destinations
                        .into_iter()
                        .map(|d| {
                            let weight = u16::try_from(d.weight).map_err(|_| {
                                anyhow::anyhow!("destination weight exceeds u16::MAX")
                            })?;
                            let weight = NonZeroU16::new(weight)
                                .ok_or_else(|| anyhow::anyhow!("destination weight must be > 0"))?;
                            Ok(pavis_core::Destination {
                                upstream: pavis_core::UpstreamName(d.upstream),
                                weight: pavis_core::Weight(weight),
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    CoreRouteAction::Forward(dests)
                }
                CodecRouteAction::Redirect { status, location } => {
                    CoreRouteAction::Redirect { status, location }
                }
                CodecRouteAction::Direct { status, body } => {
                    CoreRouteAction::Direct { status, body }
                }
            };

            let timeout = match p.timeout {
                Some(d) => {
                    let ms = u32::try_from(d.as_millis())
                        .map_err(|_| anyhow::anyhow!("timeout exceeds u32::MAX ms"))?;
                    let ms = NonZeroU32::new(ms)
                        .ok_or_else(|| anyhow::anyhow!("timeout must be > 0"))?;
                    pavis_core::Timeout::Enabled(pavis_core::Duration(ms))
                }
                None => pavis_core::Timeout::Disabled,
            };

            let retry = if let Some(r) = p.retry {
                convert_retry_policy(r, &timeout, vh_index, path_index)?
            } else {
                pavis_core::RetryPolicy::Disabled
            };

            let rewrite = match p.rewrite {
                None => pavis_core::Rewrite {
                    path: pavis_core::RewritePath::Disabled,
                    host: pavis_core::RewriteHost::Disabled,
                },
                Some(r) => {
                    let path = match r.path {
                        Some(to) => {
                            let from = matcher_path(&matcher_cfg);
                            pavis_core::RewritePath::Prefix {
                                from: pavis_core::Path(from),
                                to: pavis_core::Path(to),
                            }
                        }
                        None => pavis_core::RewritePath::Disabled,
                    };
                    let host = match r.host {
                        Some(host) => pavis_core::RewriteHost::Literal {
                            host: pavis_core::Hostname(host),
                        },
                        None => pavis_core::RewriteHost::Disabled,
                    };
                    pavis_core::Rewrite { path, host }
                }
            };

            let matcher = match matcher_cfg {
                Matcher {
                    path: PathMatcher::Prefix { path },
                    method,
                    methods,
                    headers,
                } => to_runtime_matcher(
                    pavis_core::PathMatch::Prefix {
                        path: pavis_core::Path(path),
                    },
                    method,
                    methods,
                    headers,
                    vh_index,
                    path_index,
                )?,
                Matcher {
                    path: PathMatcher::Exact { path },
                    method,
                    methods,
                    headers,
                } => to_runtime_matcher(
                    pavis_core::PathMatch::Exact {
                        path: pavis_core::Path(path),
                    },
                    method,
                    methods,
                    headers,
                    vh_index,
                    path_index,
                )?,
                Matcher {
                    path: PathMatcher::Regex { path },
                    method,
                    methods,
                    headers,
                } => to_runtime_matcher(
                    pavis_core::PathMatch::Regex {
                        path: pavis_core::Path(path),
                    },
                    method,
                    methods,
                    headers,
                    vh_index,
                    path_index,
                )?,
            };

            let principal = match p.principal {
                None => pavis_core::Principal::Any,
                Some(PrincipalConfig::Any) => pavis_core::Principal::Any,
                Some(PrincipalConfig::Authenticated { spiffe }) => {
                    pavis_core::Principal::Authenticated { spiffe }
                }
                Some(PrincipalConfig::Prefix { prefix }) => {
                    if prefix.is_empty() {
                        anyhow::bail!("principal.prefix must not be empty");
                    }
                    pavis_core::Principal::Prefix { prefix }
                }
            };

            paths.push(pavis_core::Route {
                matcher,
                timeout,
                retry,
                request_headers: request_headers.into(),
                response_headers: response_headers.into(),
                rewrite,
                action,
                principal,
            });
        }

        runtime_routes.push(pavis_core::VirtualHost {
            host: pavis_core::Host(v.host),
            paths,
        });
    }

    Ok(runtime_routes)
}

pub(super) fn from_runtime(routes: Vec<pavis_core::VirtualHost>) -> Result<Vec<VirtualHost>> {
    let mut serde_routes = Vec::new();

    for v in routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let request_headers = from_runtime_headers(&p.request_headers);
            let response_headers = from_runtime_headers(&p.response_headers);

            let action = match p.action {
                CoreRouteAction::Forward(destinations) => {
                    let dests = destinations
                        .into_iter()
                        .map(|d| WeightedDestination {
                            upstream: d.upstream.0,
                            weight: d.weight.0.get() as u32,
                        })
                        .collect();
                    CodecRouteAction::Forward {
                        destinations: dests,
                    }
                }
                CoreRouteAction::Redirect { status, location } => {
                    CodecRouteAction::Redirect { status, location }
                }
                CoreRouteAction::Direct { status, body } => {
                    CodecRouteAction::Direct { status, body }
                }
                #[allow(unreachable_patterns)]
                _ => {
                    return Err(anyhow::anyhow!("unknown route action variant"));
                }
            };

            let timeout = match p.timeout {
                pavis_core::Timeout::Disabled => None,
                pavis_core::Timeout::Enabled(d) => {
                    Some(std::time::Duration::from_millis(d.0.get() as u64))
                }
                #[allow(unreachable_patterns)]
                _ => None,
            };

            let retry = match p.retry {
                pavis_core::RetryPolicy::Disabled => None,
                pavis_core::RetryPolicy::Enabled {
                    max_attempts,
                    per_try,
                    retryable_reasons,
                    retryable_status_codes,
                    backoff,
                    retry_non_idempotent,
                    fail_on_non_replayable_retry,
                    max_request_body_buffer_bytes,
                } => {
                    use crate::config::types::BackoffStrategyDTO;

                    let reasons: Vec<String> = retryable_reasons
                        .iter()
                        .map(|r| match r {
                            pavis_core::RetryReason::StatusCode => "status_code".to_string(),
                            pavis_core::RetryReason::ConnectTimeout => {
                                "connect_timeout".to_string()
                            }
                            pavis_core::RetryReason::ReadTimeout => "read_timeout".to_string(),
                            pavis_core::RetryReason::PerTryTimeout => "per_try_timeout".to_string(),
                            pavis_core::RetryReason::PoolFull => "pool_full".to_string(),
                            pavis_core::RetryReason::ConnectError => "connect_error".to_string(),
                        })
                        .collect();

                    let codes = retryable_status_codes.as_ref().map(|c| c.codes.clone());

                    let backoff_dto = match backoff {
                        pavis_core::BackoffStrategy::Fixed { base_ms } => {
                            BackoffStrategyDTO::Fixed { base_ms }
                        }
                        pavis_core::BackoffStrategy::Linear { base_ms } => {
                            BackoffStrategyDTO::Linear { base_ms }
                        }
                        pavis_core::BackoffStrategy::Exponential { base_ms, max_ms } => {
                            BackoffStrategyDTO::Exponential { base_ms, max_ms }
                        }
                        _ => BackoffStrategyDTO::Fixed { base_ms: 100 },
                    };

                    let per_try_dto = match per_try {
                        pavis_core::TryTimeout::Enabled(d) => {
                            Some(std::time::Duration::from_millis(d.0.get() as u64))
                        }
                        _ => None,
                    };

                    Some(RetryPolicy {
                        max_attempts: max_attempts.get(),
                        retryable_reasons: reasons,
                        retryable_status_codes: codes,
                        backoff: backoff_dto,
                        retry_non_idempotent,
                        fail_on_non_replayable_retry,
                        max_request_body_buffer_bytes,
                        per_try: per_try_dto,
                    })
                }
                #[allow(unreachable_patterns)]
                _ => None,
            };

            let rewrite = match p.rewrite {
                pavis_core::Rewrite {
                    path: pavis_core::RewritePath::Disabled,
                    host: pavis_core::RewriteHost::Disabled,
                } => None,
                pavis_core::Rewrite { path, host } => Some(RewritePolicy {
                    path: match path {
                        pavis_core::RewritePath::Prefix { to, .. } => Some(to.0),
                        pavis_core::RewritePath::Disabled => None,
                        #[allow(unreachable_patterns)]
                        _ => None,
                    },
                    host: match host {
                        pavis_core::RewriteHost::Literal { host } => Some(host.0),
                        pavis_core::RewriteHost::Disabled => None,
                        #[allow(unreachable_patterns)]
                        _ => None,
                    },
                }),
            };

            let matcher = match p.matcher.path {
                pavis_core::PathMatch::Prefix { path } => Matcher {
                    path: PathMatcher::Prefix { path: path.0 },
                    method: from_runtime_method(&p.matcher.method),
                    methods: from_runtime_methods(&p.matcher.method),
                    headers: from_runtime_headers_predicates(&p.matcher.headers),
                },
                pavis_core::PathMatch::Exact { path } => Matcher {
                    path: PathMatcher::Exact { path: path.0 },
                    method: from_runtime_method(&p.matcher.method),
                    methods: from_runtime_methods(&p.matcher.method),
                    headers: from_runtime_headers_predicates(&p.matcher.headers),
                },
                pavis_core::PathMatch::Regex { path } => Matcher {
                    path: PathMatcher::Regex { path: path.0 },
                    method: from_runtime_method(&p.matcher.method),
                    methods: from_runtime_methods(&p.matcher.method),
                    headers: from_runtime_headers_predicates(&p.matcher.headers),
                },
                #[allow(unreachable_patterns)]
                _ => {
                    return Err(anyhow::anyhow!("unknown path match variant"));
                }
            };

            let principal = match p.principal {
                pavis_core::Principal::Any => None,
                pavis_core::Principal::Authenticated { spiffe } => {
                    Some(PrincipalConfig::Authenticated { spiffe })
                }
                pavis_core::Principal::Prefix { prefix } => {
                    Some(PrincipalConfig::Prefix { prefix })
                }
                #[allow(unreachable_patterns)]
                _ => None,
            };

            paths.push(Route {
                matcher: Some(matcher),
                timeout,
                retry,
                request_headers,
                response_headers,
                rewrite,
                action,
                principal,
            });
        }

        serde_routes.push(VirtualHost {
            host: v.host.0,
            paths,
        });
    }

    Ok(serde_routes)
}

fn default_matcher() -> Matcher {
    Matcher {
        path: PathMatcher::Prefix {
            path: "/".to_string(),
        },
        method: None,
        methods: None,
        headers: None,
    }
}

fn matcher_path(matcher: &Matcher) -> String {
    match &matcher.path {
        PathMatcher::Prefix { path } => path.clone(),
        PathMatcher::Exact { path } => path.clone(),
        PathMatcher::Regex { path } => path.clone(),
    }
}

/// Convert DTO matcher to core RouteMatcher with default materialization.
fn to_runtime_matcher(
    path: pavis_core::PathMatch,
    method: Option<String>,
    methods: Option<Vec<String>>,
    headers: Option<Vec<HeaderPredicate>>,
    vhost_index: usize,
    path_index: usize,
) -> Result<pavis_core::RouteMatcher> {
    let method_field_path = route_method_field_path(vhost_index, path_index);

    // Materialize method predicate (default: Any)
    // Priority: methods (List) > method (Specific) > Any
    let method = if let Some(list) = methods {
        let mut core_list = Vec::with_capacity(list.len());
        for (i, m) in list.into_iter().enumerate() {
            let path = format!("{}[{}]", method_field_path, i);
            core_list.push(parse_http_method(&m, path)?);
        }
        pavis_core::MethodPredicate::List(core_list)
    } else if let Some(m) = method {
        let http_method = parse_http_method(&m, method_field_path)?;
        pavis_core::MethodPredicate::Specific(http_method)
    } else {
        pavis_core::MethodPredicate::Any
    };

    // Materialize header predicates (default: None)
    let headers = match headers {
        None => pavis_core::HeaderPredicates::None,
        Some(preds) if preds.is_empty() => pavis_core::HeaderPredicates::None,
        Some(preds) => {
            let core_preds = preds
                .into_iter()
                .enumerate()
                .map(|(header_index, predicate)| {
                    to_runtime_header_predicate(predicate, vhost_index, path_index, header_index)
                })
                .collect::<Result<Vec<_>>>()?;
            pavis_core::HeaderPredicates::Some(core_preds)
        }
    };

    Ok(pavis_core::RouteMatcher {
        path,
        method,
        headers,
    })
}

/// Parse HTTP method string (case-insensitive).
fn parse_http_method(method: &str, field_path: String) -> Result<pavis_core::HttpMethod> {
    match method.to_uppercase().as_str() {
        "GET" => Ok(pavis_core::HttpMethod::GET),
        "POST" => Ok(pavis_core::HttpMethod::POST),
        "PUT" => Ok(pavis_core::HttpMethod::PUT),
        "DELETE" => Ok(pavis_core::HttpMethod::DELETE),
        "PATCH" => Ok(pavis_core::HttpMethod::PATCH),
        "HEAD" => Ok(pavis_core::HttpMethod::HEAD),
        "OPTIONS" => Ok(pavis_core::HttpMethod::OPTIONS),
        "CONNECT" => Ok(pavis_core::HttpMethod::CONNECT),
        "TRACE" => Ok(pavis_core::HttpMethod::TRACE),
        _ => Err(invalid_config_error(
            format!(
                "invalid HTTP method '{}' (supported: GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS, CONNECT, TRACE)",
                method
            ),
            Some(field_path),
            Some("valid_http_method"),
        )),
    }
}

fn route_match_field_path(vhost_index: usize, path_index: usize) -> FieldPathBuilder {
    FieldPathBuilder::new()
        .root("routes")
        .index(vhost_index)
        .field("paths")
        .index(path_index)
        .field("match")
}

fn route_method_field_path(vhost_index: usize, path_index: usize) -> String {
    route_match_field_path(vhost_index, path_index)
        .field("method")
        .finish()
}

fn route_headers_field_path(vhost_index: usize, path_index: usize) -> FieldPathBuilder {
    route_match_field_path(vhost_index, path_index).field("headers")
}

fn header_field_path(
    vhost_index: usize,
    path_index: usize,
    header_name: Option<&str>,
    header_index: usize,
) -> String {
    let builder = route_headers_field_path(vhost_index, path_index);
    match header_name {
        Some(name) if !name.is_empty() => builder.map_key(name).finish(),
        _ => builder.index(header_index).finish(),
    }
}

fn invalid_config_error(
    message: impl Into<String>,
    field_path: Option<String>,
    constraint: Option<&str>,
) -> anyhow::Error {
    let err = PavisError::new(ErrorCode::InvalidConfig, message);
    let err = err.with_context(|ctx| {
        let mut ctx = ctx;
        if let Some(path) = field_path {
            ctx = ctx.with_field_path(path);
        }
        if let Some(code) = constraint {
            ctx = ctx.with_constraint(code.to_string());
        }
        ctx
    });
    anyhow::Error::new(err)
}

/// Convert DTO header predicate to core type.
fn to_runtime_header_predicate(
    pred: HeaderPredicate,
    vhost_index: usize,
    path_index: usize,
    header_index: usize,
) -> Result<pavis_core::HeaderPredicate> {
    match pred {
        HeaderPredicate::V1(legacy) => {
            to_runtime_header_predicate_legacy(legacy, vhost_index, path_index, header_index)
        }
        HeaderPredicate::V2(dto) => {
            to_runtime_header_predicate_dto(dto, vhost_index, path_index, header_index)
        }
    }
}

fn to_runtime_header_predicate_dto(
    pred: HeaderMatcherDTO,
    vhost_index: usize,
    path_index: usize,
    header_index: usize,
) -> Result<pavis_core::HeaderPredicate> {
    let (name, matcher) = match pred {
        HeaderMatcherDTO::Exact { name, value } => (
            name,
            pavis_core::HeaderMatch::Exact(CompactString::new(value)),
        ),
        HeaderMatcherDTO::Prefix { name, prefix } => (
            name,
            pavis_core::HeaderMatch::Prefix(CompactString::new(prefix)),
        ),
        HeaderMatcherDTO::Regex { name, pattern } => {
            // Validate regex
            let header_path = header_field_path(vhost_index, path_index, Some(&name), header_index);
            if pattern.len() > 256 {
                return Err(invalid_config_error(
                    format!("regex pattern for header '{}' exceeds 256 bytes", name),
                    Some(header_path.clone()),
                    Some("regex_pattern_too_long"),
                ));
            }
            regex::Regex::new(&pattern).map_err(|e| {
                invalid_config_error(
                    format!("invalid regex pattern for header '{}': {}", name, e),
                    Some(header_path),
                    Some("regex_invalid_syntax"),
                )
            })?;
            (
                name,
                pavis_core::HeaderMatch::Regex(CompactString::new(pattern)),
            )
        }
        HeaderMatcherDTO::Present { name } => (name, pavis_core::HeaderMatch::Present),
        HeaderMatcherDTO::Absent { name } => (name, pavis_core::HeaderMatch::Absent),
    };

    validate_header_name(&name, vhost_index, path_index, header_index)?;

    Ok(pavis_core::HeaderPredicate {
        name: CompactString::new(name.to_ascii_lowercase()),
        matcher,
    })
}

fn validate_header_name(
    name: &str,
    vhost_index: usize,
    path_index: usize,
    header_index: usize,
) -> Result<()> {
    if name.is_empty() {
        return Err(invalid_config_error(
            "header predicate name cannot be empty",
            Some(header_field_path(
                vhost_index,
                path_index,
                None,
                header_index,
            )),
            Some("header_name_non_empty"),
        ));
    }
    if !name
        .chars()
        .all(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(invalid_config_error(
            format!(
                "invalid header name '{}' (must contain only alphanumeric, -, or _)",
                name
            ),
            Some(header_field_path(
                vhost_index,
                path_index,
                Some(name),
                header_index,
            )),
            Some("header_name_invalid"),
        ));
    }
    Ok(())
}

fn to_runtime_header_predicate_legacy(
    pred: HeaderPredicateLegacy,
    vhost_index: usize,
    path_index: usize,
    header_index: usize,
) -> Result<pavis_core::HeaderPredicate> {
    let canonical_name = pred.name.to_ascii_lowercase();
    let header_path =
        header_field_path(vhost_index, path_index, Some(&canonical_name), header_index);

    validate_header_name(&pred.name, vhost_index, path_index, header_index)?;

    let matcher = if pred.absent {
        // Absent takes precedence
        if pred.value.is_some() || pred.regex || pred.prefix {
            return Err(invalid_config_error(
                format!(
                    "header predicate '{}': absent=true is incompatible with value, regex, or prefix",
                    pred.name
                ),
                Some(header_path.clone()),
                Some("header_absent_incompatible"),
            ));
        }
        pavis_core::HeaderMatch::Absent
    } else {
        match (&pred.value, pred.regex, pred.prefix) {
            (None, false, false) => pavis_core::HeaderMatch::Present,
            (Some(val), false, false) => pavis_core::HeaderMatch::Exact(CompactString::new(val)),
            (Some(prefix_val), false, true) => {
                pavis_core::HeaderMatch::Prefix(CompactString::new(prefix_val))
            }
            (Some(pattern), true, false) => {
                // Validate regex pattern syntax at codec time (static validation only - no compilation)
                if pattern.len() > 256 {
                    return Err(invalid_config_error(
                        format!("regex pattern for header '{}' exceeds 256 bytes", pred.name),
                        Some(header_path.clone()),
                        Some("regex_pattern_too_long"),
                    ));
                }
                regex::Regex::new(pattern).map_err(|e| {
                    invalid_config_error(
                        format!("invalid regex pattern for header '{}': {}", pred.name, e),
                        Some(header_path.clone()),
                        Some("regex_invalid_syntax"),
                    )
                })?;
                pavis_core::HeaderMatch::Regex(CompactString::new(pattern))
            }
            (None, true, _) => {
                return Err(invalid_config_error(
                    format!(
                        "header predicate '{}': regex=true requires a value",
                        pred.name
                    ),
                    Some(header_path.clone()),
                    Some("regex_requires_value"),
                ));
            }
            (None, _, true) => {
                return Err(invalid_config_error(
                    format!(
                        "header predicate '{}': prefix=true requires a value",
                        pred.name
                    ),
                    Some(header_path.clone()),
                    Some("prefix_requires_value"),
                ));
            }
            (Some(_), true, true) => {
                return Err(invalid_config_error(
                    format!(
                        "header predicate '{}': regex and prefix are mutually exclusive",
                        pred.name
                    ),
                    Some(header_path.clone()),
                    Some("regex_prefix_exclusive"),
                ));
            }
        }
    };

    Ok(pavis_core::HeaderPredicate {
        name: CompactString::new(&canonical_name),
        matcher,
    })
}

/// Convert core method predicate to DTO (None if Any).
fn from_runtime_method(method: &pavis_core::MethodPredicate) -> Option<String> {
    match method {
        pavis_core::MethodPredicate::Any => None,
        pavis_core::MethodPredicate::Specific(m) => Some(m.as_str().to_string()),
        pavis_core::MethodPredicate::List(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// Convert core method predicate to DTO list (None if not List).
fn from_runtime_methods(method: &pavis_core::MethodPredicate) -> Option<Vec<String>> {
    match method {
        pavis_core::MethodPredicate::List(list) => {
            Some(list.iter().map(|m| m.as_str().to_string()).collect())
        }
        _ => None,
    }
}

/// Convert core header predicates to DTO (None if no predicates).
fn from_runtime_headers_predicates(
    headers: &pavis_core::HeaderPredicates,
) -> Option<Vec<HeaderPredicate>> {
    match headers {
        pavis_core::HeaderPredicates::None => None,
        pavis_core::HeaderPredicates::Some(preds) => {
            let dto_preds = preds.iter().map(from_runtime_header_predicate).collect();
            Some(dto_preds)
        }
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// Convert core header predicate to DTO.
fn from_runtime_header_predicate(pred: &pavis_core::HeaderPredicate) -> HeaderPredicate {
    let legacy = match &pred.matcher {
        pavis_core::HeaderMatch::Present => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: None,
            regex: false,
            prefix: false,
            absent: false,
        },
        pavis_core::HeaderMatch::Exact(val) => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: Some(val.to_string()),
            regex: false,
            prefix: false,
            absent: false,
        },
        pavis_core::HeaderMatch::Prefix(prefix_val) => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: Some(prefix_val.to_string()),
            regex: false,
            prefix: true,
            absent: false,
        },
        pavis_core::HeaderMatch::Regex(pattern) => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: Some(pattern.to_string()),
            regex: true,
            prefix: false,
            absent: false,
        },
        pavis_core::HeaderMatch::Absent => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: None,
            regex: false,
            prefix: false,
            absent: true,
        },
        #[allow(unreachable_patterns)]
        _ => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: None,
            regex: false,
            prefix: false,
            absent: false,
        },
    };
    HeaderPredicate::V1(legacy)
}

fn to_runtime_headers(h: Option<HeaderOperations>) -> pavis_core::HeadersPolicy {
    match h {
        None => pavis_core::HeadersPolicy::Disabled,
        Some(h) => pavis_core::HeadersPolicy::Enabled {
            rules: pavis_core::Headers {
                set_headers: h
                    .set_headers
                    .into_iter()
                    .map(|(k, v)| (pavis_core::HeaderName(k), pavis_core::HeaderValue(v)))
                    .collect(),
                append_headers: h
                    .append_headers
                    .into_iter()
                    .map(|(k, v)| (pavis_core::HeaderName(k), pavis_core::HeaderValue(v)))
                    .collect(),
                add_headers: h
                    .add_headers
                    .into_iter()
                    .map(|(k, v)| (pavis_core::HeaderName(k), pavis_core::HeaderValue(v)))
                    .collect(),
                remove_headers: h
                    .remove_headers
                    .into_iter()
                    .map(pavis_core::HeaderName)
                    .collect(),
            },
        },
    }
}

/// Convert retry policy DTO to core type with full P2 validation
fn convert_retry_policy(
    dto: RetryPolicy,
    route_timeout: &pavis_core::Timeout,
    vh_index: usize,
    path_index: usize,
) -> Result<pavis_core::RetryPolicy> {
    use crate::config::types::BackoffStrategyDTO;

    let field_path_base = format!("routes[{}].paths[{}].retry", vh_index, path_index);

    // Validate max_attempts range: 1..=10
    if dto.max_attempts == 0 {
        return Err(invalid_config_error(
            "max_attempts must be >= 1",
            Some(format!("{}.max_attempts", field_path_base)),
            Some("min_value=1"),
        ));
    }
    if dto.max_attempts > 10 {
        return Err(invalid_config_error(
            format!("max_attempts {} exceeds maximum of 10", dto.max_attempts),
            Some(format!("{}.max_attempts", field_path_base)),
            Some("max_value=10"),
        ));
    }

    let max_attempts = NonZeroU16::new(dto.max_attempts)
        .ok_or_else(|| anyhow::anyhow!("max_attempts must be > 0"))?;

    // Parse retryable reasons
    let mut retryable_reasons = Vec::new();
    for reason_str in &dto.retryable_reasons {
        let reason = match reason_str.as_str() {
            "status_code" => pavis_core::RetryReason::StatusCode,
            "connect_timeout" => pavis_core::RetryReason::ConnectTimeout,
            "read_timeout" => pavis_core::RetryReason::ReadTimeout,
            "per_try_timeout" => pavis_core::RetryReason::PerTryTimeout,
            "pool_full" => pavis_core::RetryReason::PoolFull,
            "connect_error" => pavis_core::RetryReason::ConnectError,
            unknown => {
                return Err(invalid_config_error(
                    format!("unknown retryable reason: '{}'", unknown),
                    Some(format!("{}.retryable_reasons", field_path_base)),
                    Some("valid_retry_reason"),
                ));
            }
        };
        retryable_reasons.push(reason);
    }

    // Validate retryable_status_codes is present when status_code is in retryable_reasons
    let has_status_code_reason = retryable_reasons
        .iter()
        .any(|r| matches!(r, pavis_core::RetryReason::StatusCode));

    let retryable_status_codes = if has_status_code_reason {
        match dto.retryable_status_codes {
            None => {
                return Err(invalid_config_error(
                    "retryable_status_codes is required when 'status_code' is in retryable_reasons",
                    Some(format!("{}.retryable_status_codes", field_path_base)),
                    Some("required_when_status_code_retryable"),
                ));
            }
            Some(codes) if codes.is_empty() => {
                return Err(invalid_config_error(
                    "retryable_status_codes cannot be empty when 'status_code' is in retryable_reasons",
                    Some(format!("{}.retryable_status_codes", field_path_base)),
                    Some("required_when_status_code_retryable"),
                ));
            }
            Some(codes) => Some(pavis_core::RetryableStatusCodes { codes }),
        }
    } else {
        dto.retryable_status_codes
            .map(|codes| pavis_core::RetryableStatusCodes { codes })
    };

    // Convert backoff strategy
    let backoff = match dto.backoff {
        BackoffStrategyDTO::Fixed { base_ms } => pavis_core::BackoffStrategy::Fixed { base_ms },
        BackoffStrategyDTO::Linear { base_ms } => pavis_core::BackoffStrategy::Linear { base_ms },
        BackoffStrategyDTO::Exponential { base_ms, max_ms } => {
            pavis_core::BackoffStrategy::Exponential { base_ms, max_ms }
        }
    };

    let per_try = match dto.per_try {
        Some(d) => {
            let per_try_ms = d.as_millis() as u32;
            let request_timeout_ms = match route_timeout {
                pavis_core::Timeout::Enabled(d) => d.0.get(),
                pavis_core::Timeout::Disabled => 60000, // Default 60s
                #[allow(unreachable_patterns)]
                _ => 60000,
            };

            if per_try_ms > request_timeout_ms {
                return Err(invalid_config_error(
                    format!(
                        "per_try timeout ({}ms) exceeds overall route timeout ({}ms)",
                        per_try_ms, request_timeout_ms
                    ),
                    Some(format!("{}.per_try", field_path_base)),
                    Some("per_try_timeout_lte_request_timeout"),
                ));
            }

            pavis_core::TryTimeout::Enabled(pavis_core::Duration(
                std::num::NonZeroU32::new(per_try_ms).unwrap_or(std::num::NonZeroU32::MIN),
            ))
        }
        None => pavis_core::TryTimeout::Inherit,
    };

    Ok(pavis_core::RetryPolicy::Enabled {
        max_attempts,
        per_try,
        retryable_reasons,
        retryable_status_codes,
        backoff,
        retry_non_idempotent: dto.retry_non_idempotent,
        fail_on_non_replayable_retry: dto.fail_on_non_replayable_retry,
        max_request_body_buffer_bytes: dto.max_request_body_buffer_bytes,
    })
}

fn from_runtime_headers(h: &pavis_core::HeadersPolicy) -> Option<HeaderOperations> {
    match h {
        pavis_core::HeadersPolicy::Disabled => None,
        pavis_core::HeadersPolicy::Enabled { rules } => Some(HeaderOperations {
            set_headers: rules
                .set_headers
                .iter()
                .map(|(k, v)| (k.0.clone(), v.0.clone()))
                .collect(),
            append_headers: rules
                .append_headers
                .iter()
                .map(|(k, v)| (k.0.clone(), v.0.clone()))
                .collect(),
            add_headers: rules
                .add_headers
                .iter()
                .map(|(k, v)| (k.0.clone(), v.0.clone()))
                .collect(),
            remove_headers: rules.remove_headers.iter().map(|k| k.0.clone()).collect(),
        }),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{
        BackoffStrategyDTO, Matcher, PathMatcher, RetryPolicy, Route,
        RouteAction as CodecRouteAction, VirtualHost, WeightedDestination,
    };
    use std::time::Duration;

    #[test]
    fn to_runtime_validates_destination_weight() {
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher {
                    path: PathMatcher::Prefix {
                        path: "/".to_string(),
                    },
                    method: None,
                    methods: None,
                    headers: None,
                }),
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: None,
                action: CodecRouteAction::Forward {
                    destinations: vec![WeightedDestination {
                        upstream: "u1".to_string(),
                        weight: 0,
                    }],
                },
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(err.to_string().contains("destination weight must be > 0"));

        // test weight overflow
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher {
                    path: PathMatcher::Prefix {
                        path: "/".to_string(),
                    },
                    method: None,
                    methods: None,
                    headers: None,
                }),
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: None,
                action: CodecRouteAction::Forward {
                    destinations: vec![WeightedDestination {
                        upstream: "u1".to_string(),
                        weight: 70000,
                    }],
                },
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(
            err.to_string()
                .contains("destination weight exceeds u16::MAX")
        );
    }

    #[test]
    fn principal_conversion_variants() {
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![
                Route {
                    matcher: None,
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    principal: Some(PrincipalConfig::Any),
                    rewrite: None,
                    action: CodecRouteAction::Forward {
                        destinations: vec![],
                    },
                },
                Route {
                    matcher: None,
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    principal: Some(PrincipalConfig::Prefix {
                        prefix: "".to_string(),
                    }),
                    rewrite: None,
                    action: CodecRouteAction::Forward {
                        destinations: vec![],
                    },
                },
            ],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(
            err.to_string()
                .contains("principal.prefix must not be empty")
        );
    }

    #[test]
    fn to_runtime_validates_timeout_limits() {
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher {
                    path: PathMatcher::Prefix {
                        path: "/".to_string(),
                    },
                    method: None,
                    methods: None,
                    headers: None,
                }),
                timeout: Some(Duration::from_millis(u64::MAX)), // Exceeds u32
                retry: None,
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: None,
                action: CodecRouteAction::Forward {
                    destinations: vec![],
                },
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(err.to_string().contains("timeout exceeds u32::MAX"));
    }

    #[test]
    fn to_runtime_validates_retry_policy() {
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher {
                    path: PathMatcher::Prefix {
                        path: "/".to_string(),
                    },
                    method: None,
                    methods: None,
                    headers: None,
                }),
                timeout: None,
                retry: Some(RetryPolicy {
                    max_attempts: 0, // Invalid - should trigger error
                    retryable_reasons: vec![],
                    retryable_status_codes: None,
                    backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
                    retry_non_idempotent: false,
                    fail_on_non_replayable_retry: false,
                    max_request_body_buffer_bytes: 1_048_576,
                    per_try: Some(Duration::from_secs(1)),
                }),
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: None,
                action: CodecRouteAction::Forward {
                    destinations: vec![],
                },
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(err.to_string().contains("max_attempts must be >= 1"));

        // Test attempts overflow
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: Some(RetryPolicy {
                    max_attempts: 11, // Exceeds max of 10
                    retryable_reasons: vec![],
                    retryable_status_codes: None,
                    backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
                    retry_non_idempotent: false,
                    fail_on_non_replayable_retry: false,
                    max_request_body_buffer_bytes: 1_048_576,
                    per_try: Some(Duration::from_secs(1)),
                }),
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: None,
                action: CodecRouteAction::Forward {
                    destinations: vec![],
                },
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(err.to_string().contains("exceeds maximum of 10"));
    }

    #[test]
    fn from_runtime_handles_timeouts_and_retries() {
        use pavis_core::*;
        let runtime_vhost = pavis_core::VirtualHost {
            host: Host("example.com".to_string()),
            paths: vec![pavis_core::Route {
                matcher: RouteMatcher {
                    path: PathMatch::Prefix {
                        path: Path("/".to_string()),
                    },
                    method: MethodPredicate::Any,
                    headers: HeaderPredicates::None,
                },
                timeout: Timeout::Enabled(pavis_core::Duration(NonZeroU32::new(5000).unwrap())),
                retry: RetryPolicy::Enabled {
                    max_attempts: NonZeroU16::new(3).unwrap(),
                    per_try: pavis_core::TryTimeout::Inherit,
                    retryable_reasons: vec![pavis_core::RetryReason::StatusCode],
                    retryable_status_codes: Some(pavis_core::RetryableStatusCodes {
                        codes: vec![502, 503, 504],
                    }),
                    backoff: pavis_core::BackoffStrategy::Exponential {
                        base_ms: 100,
                        max_ms: 5000,
                    },
                    retry_non_idempotent: false,
                    fail_on_non_replayable_retry: false,
                    max_request_body_buffer_bytes: 1_048_576,
                },
                request_headers: HeadersPolicy::Disabled.into(),
                response_headers: HeadersPolicy::Disabled.into(),
                principal: pavis_core::Principal::Authenticated {
                    spiffe: "s".to_string(),
                },
                rewrite: Rewrite {
                    path: RewritePath::Disabled,
                    host: RewriteHost::Disabled,
                },
                action: CoreRouteAction::Forward(vec![]),
            }],
        };

        let serde_vhost = from_runtime(vec![runtime_vhost]).expect("from_runtime");
        let route = &serde_vhost[0].paths[0];
        assert_eq!(route.timeout, Some(std::time::Duration::from_secs(5)));
        assert_eq!(route.retry.as_ref().unwrap().max_attempts, 3);
        assert_eq!(
            route.principal.as_ref().unwrap(),
            &PrincipalConfig::Authenticated {
                spiffe: "s".to_string()
            }
        );
    }

    #[test]
    fn to_runtime_headers_branches() {
        let ops = HeaderOperations {
            set_headers: vec![],
            append_headers: vec![],
            add_headers: vec![],
            remove_headers: vec![],
        };
        let policy = to_runtime_headers(Some(ops));
        match policy {
            pavis_core::HeadersPolicy::Enabled { rules } => {
                assert!(rules.set_headers.is_empty());
            }
            _ => panic!("expected enabled"),
        }
    }

    #[test]
    fn conversion_handles_all_variants() {
        use crate::config::types::{
            Matcher, PathMatcher, PrincipalConfig, Route, RouteAction as CodecRouteAction,
            VirtualHost,
        };
        use pavis_core::{PathMatch, Principal, RouteAction as CoreRouteAction};

        let vhost = VirtualHost {
            host: "example.com".to_string(),
            paths: vec![
                // 1. Redirect, Authenticated Principal, Exact Match
                Route {
                    matcher: Some(Matcher {
                        path: PathMatcher::Exact {
                            path: "/secure".to_string(),
                        },
                        method: None,
                        methods: None,
                        headers: None,
                    }),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    principal: Some(PrincipalConfig::Authenticated {
                        spiffe: "spiffe://example.org/ns/foo/sa/bar".to_string(),
                    }),
                    rewrite: None,
                    action: CodecRouteAction::Redirect {
                        status: 302,
                        location: "/login".to_string(),
                    },
                },
                // 2. Direct, Prefix Principal, Regex Match
                Route {
                    matcher: Some(Matcher {
                        path: PathMatcher::Regex {
                            path: "^/admin/.*".to_string(),
                        },
                        method: None,
                        methods: None,
                        headers: None,
                    }),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    principal: Some(PrincipalConfig::Prefix {
                        prefix: "admin-".to_string(),
                    }),
                    rewrite: None,
                    action: CodecRouteAction::Direct {
                        status: 403,
                        body: "forbidden".to_string(),
                    },
                },
            ],
        };

        let runtime = to_runtime(vec![vhost]).unwrap();

        // Check Runtime
        let paths = &runtime[0].paths;
        match &paths[0].action {
            CoreRouteAction::Redirect { status, location } => {
                assert_eq!(*status, 302);
                assert_eq!(location, "/login");
            }
            _ => panic!("expected redirect"),
        }
        match &paths[0].principal {
            Principal::Authenticated { spiffe } => {
                assert_eq!(spiffe, "spiffe://example.org/ns/foo/sa/bar")
            }
            _ => panic!("expected authenticated"),
        }
        match &paths[0].matcher.path {
            PathMatch::Exact { path } => assert_eq!(path.0, "/secure"),
            _ => panic!("expected exact match"),
        }

        match &paths[1].action {
            CoreRouteAction::Direct { status, body } => {
                assert_eq!(*status, 403);
                assert_eq!(body, "forbidden");
            }
            _ => panic!("expected direct"),
        }
        match &paths[1].principal {
            Principal::Prefix { prefix } => assert_eq!(prefix, "admin-"),
            _ => panic!("expected prefix"),
        }
        match &paths[1].matcher.path {
            PathMatch::Regex { path } => assert_eq!(path.0, "^/admin/.*"),
            _ => panic!("expected regex match"),
        }

        // Round trip back
        let serde_back = from_runtime(runtime).expect("from_runtime");
        let paths_back = &serde_back[0].paths;

        match &paths_back[0].action {
            CodecRouteAction::Redirect { status, location } => {
                assert_eq!(*status, 302);
                assert_eq!(location, "/login");
            }
            _ => panic!("expected redirect back"),
        }
        match &paths_back[0].principal {
            Some(PrincipalConfig::Authenticated { spiffe }) => {
                assert_eq!(spiffe, "spiffe://example.org/ns/foo/sa/bar")
            }
            _ => panic!("expected authenticated back"),
        }

        match &paths_back[1].action {
            CodecRouteAction::Direct { status, body } => {
                assert_eq!(*status, 403);
                assert_eq!(body, "forbidden");
            }
            _ => panic!("expected direct back"),
        }
        match &paths_back[1].principal {
            Some(PrincipalConfig::Prefix { prefix }) => assert_eq!(prefix, "admin-"),
            _ => panic!("expected prefix back"),
        }
    }

    #[test]
    fn from_runtime_headers_round_trip() {
        let headers = pavis_core::Headers {
            set_headers: vec![(
                pavis_core::HeaderName("x-set".to_string()),
                pavis_core::HeaderValue("v1".to_string()),
            )],
            append_headers: vec![(
                pavis_core::HeaderName("x-append".to_string()),
                pavis_core::HeaderValue("v2".to_string()),
            )],
            add_headers: vec![(
                pavis_core::HeaderName("x-add".to_string()),
                pavis_core::HeaderValue("v3".to_string()),
            )],
            remove_headers: vec![pavis_core::HeaderName("x-remove".to_string())],
        };
        let policy = pavis_core::HeadersPolicy::Enabled { rules: headers };

        let ops = from_runtime_headers(&policy).unwrap();
        assert_eq!(ops.set_headers.len(), 1);
        assert_eq!(ops.append_headers.len(), 1);
        assert_eq!(ops.add_headers.len(), 1);
        assert_eq!(ops.remove_headers.len(), 1);
        assert_eq!(ops.set_headers[0].0, "x-set");
    }

    #[test]
    fn rewrite_policy_conversion() {
        use crate::config::types::{RewritePolicy, Route, RouteAction, VirtualHost};
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: Some(RewritePolicy {
                    path: Some("/new".to_string()),
                    host: Some("new.host".to_string()),
                }),
                action: RouteAction::Direct {
                    status: 200,
                    body: "".to_string(),
                },
            }],
        };

        let runtime = to_runtime(vec![vhost]).unwrap();
        let rewrite = &runtime[0].paths[0].rewrite;
        match &rewrite.path {
            pavis_core::RewritePath::Prefix { to, .. } => assert_eq!(to.0, "/new"),
            _ => panic!("expected prefix rewrite"),
        }
        match &rewrite.host {
            pavis_core::RewriteHost::Literal { host } => assert_eq!(host.0, "new.host"),
            _ => panic!("expected host rewrite"),
        }
    }

    #[test]
    fn test_retry_status_code_validation() {
        use crate::config::types::{BackoffStrategyDTO, Route, RouteAction, VirtualHost};

        // 1. Missing retryable_status_codes when status_code reason present
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: Some(RetryPolicy {
                    max_attempts: 3,
                    retryable_reasons: vec!["status_code".to_string()],
                    retryable_status_codes: None,
                    backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
                    retry_non_idempotent: false,
                    fail_on_non_replayable_retry: false,
                    max_request_body_buffer_bytes: 1024,
                    per_try: None,
                }),
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: None,
                action: RouteAction::Direct {
                    status: 200,
                    body: "".to_string(),
                },
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(
            err.to_string()
                .contains("retryable_status_codes is required")
        );

        // 2. Empty retryable_status_codes when status_code reason present
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: Some(RetryPolicy {
                    max_attempts: 3,
                    retryable_reasons: vec!["status_code".to_string()],
                    retryable_status_codes: Some(vec![]),
                    backoff: BackoffStrategyDTO::Fixed { base_ms: 100 },
                    retry_non_idempotent: false,
                    fail_on_non_replayable_retry: false,
                    max_request_body_buffer_bytes: 1024,
                    per_try: None,
                }),
                request_headers: None,
                response_headers: None,
                principal: None,
                rewrite: None,
                action: RouteAction::Direct {
                    status: 200,
                    body: "".to_string(),
                },
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(
            err.to_string()
                .contains("retryable_status_codes cannot be empty")
        );
    }
}
