use crate::config::types::{
    BackoffStrategyDTO, HeaderOperations, HeaderPredicate, HeaderPredicateLegacy, Matcher,
    PathMatcher, RetryPolicy, RewritePolicy, Route, RouteAction as CodecRouteAction, VirtualHost,
    WeightedDestination,
};
use anyhow::{Result, anyhow};
use pavis_core::{
    BackoffStrategy, HeaderMatch, HeadersPolicy, RetryReason, RouteAction as CoreRouteAction,
    TryTimeout,
};

pub fn from_runtime(routes: Vec<pavis_core::VirtualHost>) -> Result<Vec<VirtualHost>> {
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
                    return Err(anyhow!("unknown route action variant"));
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
                    let reasons: Vec<String> = retryable_reasons
                        .iter()
                        .map(|r| match r {
                            RetryReason::StatusCode => "status_code".to_string(),
                            RetryReason::ConnectTimeout => "connect_timeout".to_string(),
                            RetryReason::ReadTimeout => "read_timeout".to_string(),
                            RetryReason::PerTryTimeout => "per_try_timeout".to_string(),
                            RetryReason::PoolFull => "pool_full".to_string(),
                            RetryReason::ConnectError => "connect_error".to_string(),
                        })
                        .collect();

                    let codes = retryable_status_codes.as_ref().map(|c| c.codes.clone());

                    let backoff_dto = match backoff {
                        BackoffStrategy::Fixed { base_ms } => BackoffStrategyDTO::Fixed { base_ms },
                        BackoffStrategy::Linear { base_ms } => {
                            BackoffStrategyDTO::Linear { base_ms }
                        }
                        BackoffStrategy::Exponential { base_ms, max_ms } => {
                            BackoffStrategyDTO::Exponential { base_ms, max_ms }
                        }
                        _ => BackoffStrategyDTO::Fixed { base_ms: 100 },
                    };

                    let per_try_dto = match per_try {
                        TryTimeout::Enabled(d) => {
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
                    return Err(anyhow!("unknown path match variant"));
                }
            };

            let principal = match p.principal {
                pavis_core::Principal::Any => None,
                pavis_core::Principal::Authenticated { spiffe } => {
                    Some(crate::config::types::PrincipalConfig::Authenticated { spiffe: spiffe.0 })
                }
                pavis_core::Principal::Prefix { prefix } => {
                    Some(crate::config::types::PrincipalConfig::Prefix { prefix })
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

fn from_runtime_method(method: &pavis_core::MethodPredicate) -> Option<String> {
    match method {
        pavis_core::MethodPredicate::Any => None,
        pavis_core::MethodPredicate::Specific(m) => Some(m.as_str().to_string()),
        pavis_core::MethodPredicate::List(_) => None,
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn from_runtime_methods(method: &pavis_core::MethodPredicate) -> Option<Vec<String>> {
    match method {
        pavis_core::MethodPredicate::List(list) => {
            Some(list.iter().map(|m| m.as_str().to_string()).collect())
        }
        _ => None,
    }
}

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

fn from_runtime_header_predicate(pred: &pavis_core::HeaderPredicate) -> HeaderPredicate {
    let legacy = match &pred.matcher {
        HeaderMatch::Present => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: None,
            regex: false,
            prefix: false,
            absent: false,
        },
        HeaderMatch::Exact(val) => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: Some(val.to_string()),
            regex: false,
            prefix: false,
            absent: false,
        },
        HeaderMatch::Prefix(prefix_val) => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: Some(prefix_val.to_string()),
            regex: false,
            prefix: true,
            absent: false,
        },
        HeaderMatch::Regex(pattern) => HeaderPredicateLegacy {
            name: pred.name.to_string(),
            value: Some(pattern.to_string()),
            regex: true,
            prefix: false,
            absent: false,
        },
        HeaderMatch::Absent => HeaderPredicateLegacy {
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

fn from_runtime_headers(h: &HeadersPolicy) -> Option<HeaderOperations> {
    match h {
        HeadersPolicy::Disabled => None,
        HeadersPolicy::Enabled { rules } => Some(HeaderOperations {
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
