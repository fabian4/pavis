use anyhow::Result;

use crate::config::types::{
    HeaderOperations, RetryPolicy, Route, VirtualHost, WeightedDestination,
};

pub(super) fn to_runtime(routes: Vec<VirtualHost>) -> Result<Vec<pavis_core::VirtualHost>> {
    let mut runtime_routes = Vec::new();

    for v in routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let request_headers = if let Some(h) = p.request_headers {
                let add: Vec<(String, String)> = h.add.unwrap_or_default().into_iter().collect();
                let remove = h.remove.unwrap_or_default();
                Some(pavis_core::HeaderOperations { add, remove })
            } else {
                None
            };

            let response_headers = if let Some(h) = p.response_headers {
                let add: Vec<(String, String)> = h.add.unwrap_or_default().into_iter().collect();
                let remove = h.remove.unwrap_or_default();
                Some(pavis_core::HeaderOperations { add, remove })
            } else {
                None
            };

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

            paths.push(pavis_core::Route {
                match_type: p.match_type,
                path: p.path,
                timeout_ms,
                retry_policy,
                request_headers,
                response_headers,
                destinations,
                compiled_regex: None,
            });
        }

        runtime_routes.push(pavis_core::VirtualHost {
            host: v.host,
            paths,
        });
    }

    Ok(runtime_routes)
}

pub(super) fn from_runtime(routes: Vec<pavis_core::VirtualHost>) -> Vec<VirtualHost> {
    let mut serde_routes = Vec::new();

    for v in routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let request_headers = p.request_headers.map(|h| HeaderOperations {
                add: Some(h.add.into_iter().collect()),
                remove: Some(h.remove),
            });

            let response_headers = p.response_headers.map(|h| HeaderOperations {
                add: Some(h.add.into_iter().collect()),
                remove: Some(h.remove),
            });

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

            paths.push(Route {
                match_type: p.match_type,
                path: p.path,
                timeout,
                retry,
                request_headers,
                response_headers,
                destinations,
                compiled_regex: None,
            });
        }

        serde_routes.push(VirtualHost {
            host: v.host,
            paths,
        });
    }

    serde_routes
}
