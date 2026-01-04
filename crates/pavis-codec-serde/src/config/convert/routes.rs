use anyhow::Result;

use crate::config::types::{
    HeaderAction, HeaderOperations, RetryPolicy, RewritePolicy, Route, VirtualHost,
    WeightedDestination,
};

pub(super) fn to_runtime(routes: Vec<VirtualHost>) -> Result<Vec<pavis_core::VirtualHost>> {
    let mut runtime_routes = Vec::new();

    for v in routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let request_headers = p.request_headers.map(|h| to_runtime_headers(&h));
            let response_headers = p.response_headers.map(|h| to_runtime_headers(&h));

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| pavis_core::WeightedDestination {
                    upstream: d.upstream,
                    weight: d.weight,
                })
                .collect();

            let timeout_ms = p.timeout.map(|d| d.as_millis() as u64);
            let retry_policy = if let Some(r) = p.retry {
                let retry_on = r
                    .retry_on
                    .iter()
                    .map(|v| {
                        v.as_str().map(str::to_string).ok_or_else(|| {
                            anyhow::anyhow!("retry.retry_on entries must be strings")
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Some(pavis_core::RetryPolicy {
                    attempts: r.attempts as u32,
                    per_try_timeout_ms: r.per_try_timeout.as_millis() as u64,
                    retry_on,
                })
            } else {
                None
            };

            let rewrite = p.rewrite.map(|r| pavis_core::RewritePolicy {
                path_prefix_rewrite: r.path_prefix_rewrite,
                host_rewrite_literal: r.host_rewrite_literal,
            });

            paths.push(pavis_core::Route {
                match_type: p.match_type,
                path: p.path,
                timeout_ms,
                retry_policy,
                request_headers,
                response_headers,
                rewrite,
                destinations,
            });
        }

        runtime_routes.push(pavis_core::VirtualHost {
            host: v.host,
            paths,
        });
    }

    Ok(runtime_routes)
}

fn to_runtime_headers(h: &HeaderOperations) -> pavis_core::HeaderOperations {
    let actions = h
        .actions
        .iter()
        .map(|a| pavis_core::HeaderAction {
            key: a.key.clone(),
            value: a.value.clone(),
            action: a.action,
        })
        .collect();
    pavis_core::HeaderOperations { actions }
}

pub(super) fn from_runtime(routes: Vec<pavis_core::VirtualHost>) -> Vec<VirtualHost> {
    let mut serde_routes = Vec::new();

    for v in routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let request_headers = p.request_headers.as_ref().map(from_runtime_headers);
            let response_headers = p.response_headers.as_ref().map(from_runtime_headers);

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| WeightedDestination {
                    upstream: d.upstream,
                    weight: d.weight,
                })
                .collect();

            let timeout = p.timeout_ms.map(std::time::Duration::from_millis);
            let retry = p.retry_policy.map(|r| RetryPolicy {
                attempts: r.attempts as usize,
                per_try_timeout: std::time::Duration::from_millis(r.per_try_timeout_ms),
                retry_on: r
                    .retry_on
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            });

            let rewrite = p.rewrite.map(|r| RewritePolicy {
                path_prefix_rewrite: r.path_prefix_rewrite,
                host_rewrite_literal: r.host_rewrite_literal,
            });

            paths.push(Route {
                match_type: p.match_type,
                path: p.path,
                timeout,
                retry,
                request_headers,
                response_headers,
                rewrite,
                destinations,
            });
        }

        serde_routes.push(VirtualHost {
            host: v.host,
            paths,
        });
    }

    serde_routes
}

fn from_runtime_headers(h: &pavis_core::HeaderOperations) -> HeaderOperations {
    let actions = h
        .actions
        .iter()
        .map(|a| HeaderAction {
            key: a.key.clone(),
            value: a.value.clone(),
            action: a.action,
        })
        .collect();
    HeaderOperations { actions }
}

#[cfg(test)]
mod tests {
    use super::{from_runtime, to_runtime};
    use crate::config::types::{RetryPolicy, Route, VirtualHost, WeightedDestination};
    use pavis_core::{
        HeaderAction as RuntimeHeaderAction, HeaderActionType,
        HeaderOperations as RuntimeHeaderOperations, MatchType, Route as RuntimeRoute,
        VirtualHost as RuntimeVhost,
    };
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn to_runtime_rejects_non_string_retry_on() {
        let routes = vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Prefix,
                path: "/".to_string(),
                timeout: None,
                retry: Some(RetryPolicy {
                    attempts: 1,
                    per_try_timeout: Duration::from_millis(100),
                    retry_on: vec![json!(500)],
                }),
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: "backend".to_string(),
                    weight: 1,
                }],
            }],
        }];

        let err = to_runtime(routes).expect_err("non-string retry_on");
        assert!(
            err.to_string()
                .contains("retry.retry_on entries must be strings")
        );
    }

    #[test]
    fn from_runtime_preserves_response_headers() {
        let routes = vec![RuntimeVhost {
            host: "example.com".to_string(),
            paths: vec![RuntimeRoute {
                match_type: MatchType::Exact,
                path: "/".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: Some(RuntimeHeaderOperations {
                    actions: vec![
                        RuntimeHeaderAction {
                            key: "x-added".to_string(),
                            value: Some("1".to_string()),
                            action: HeaderActionType::Set,
                        },
                        RuntimeHeaderAction {
                            key: "x-remove".to_string(),
                            value: None,
                            action: HeaderActionType::Remove,
                        },
                    ],
                }),
                rewrite: None,
                destinations: vec![pavis_core::WeightedDestination {
                    upstream: "backend".to_string(),
                    weight: 1,
                }],
            }],
        }];

        let serde_routes = from_runtime(routes);
        let headers = serde_routes[0].paths[0]
            .response_headers
            .as_ref()
            .expect("headers");
        assert_eq!(headers.actions.len(), 2);
        assert_eq!(headers.actions[0].key, "x-added");
        assert_eq!(headers.actions[0].value.as_deref(), Some("1"));
        assert_eq!(headers.actions[0].action, HeaderActionType::Set);
        assert_eq!(headers.actions[1].key, "x-remove");
        assert!(headers.actions[1].value.is_none());
        assert_eq!(headers.actions[1].action, HeaderActionType::Remove);
    }
}
