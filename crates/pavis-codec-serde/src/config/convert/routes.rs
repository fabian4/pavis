use anyhow::Result;
use std::num::{NonZeroU16, NonZeroU32};

use crate::config::types::{
    HeaderOperations, Matcher, RetryPolicy, RewritePolicy, Route, VirtualHost, WeightedDestination,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{Matcher, RetryPolicy, Route, VirtualHost, WeightedDestination};
    use std::time::Duration;

    #[test]
    fn to_runtime_validates_destination_weight() {
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher::Prefix {
                    path: "/".to_string(),
                }),
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![WeightedDestination {
                    upstream: "u1".to_string(),
                    weight: 0,
                }],
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(err.to_string().contains("destination weight must be > 0"));
    }

    #[test]
    fn to_runtime_validates_timeout_limits() {
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher::Prefix {
                    path: "/".to_string(),
                }),
                timeout: Some(Duration::from_millis(u64::MAX)), // Exceeds u32
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![],
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
                matcher: Some(Matcher::Prefix {
                    path: "/".to_string(),
                }),
                timeout: None,
                retry: Some(RetryPolicy {
                    attempts: 0, // Invalid
                    per_try_timeout: Duration::from_secs(1),
                    retry_on: vec![],
                }),
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![],
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(err.to_string().contains("retry.attempts must be > 0"));
    }

    #[test]
    fn to_runtime_handles_retry_flags() {
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher::Prefix {
                    path: "/".to_string(),
                }),
                timeout: None,
                retry: Some(RetryPolicy {
                    attempts: 1,
                    per_try_timeout: Duration::from_secs(1),
                    retry_on: vec![
                        serde_json::Value::String("5xx".to_string()),
                        serde_json::Value::String("connect_failure".to_string()),
                        serde_json::Value::String("reset".to_string()),
                        serde_json::Value::String("refused".to_string()),
                    ],
                }),
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![],
            }],
        };
        let runtime = to_runtime(vec![vhost]).unwrap();
        let retry = match &runtime[0].paths[0].retry {
            pavis_core::RetryPolicy::Enabled { on, .. } => on,
            _ => panic!("expected enabled retry"),
        };
        assert_eq!(
            retry.0,
            pavis_core::RETRY_FIVE_XX
                | pavis_core::RETRY_CONNECT_FAILURE
                | pavis_core::RETRY_RESET
                | pavis_core::RETRY_REFUSED
        );
    }

    #[test]
    fn to_runtime_rejects_invalid_retry_flags() {
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: Some(Matcher::Prefix {
                    path: "/".to_string(),
                }),
                timeout: None,
                retry: Some(RetryPolicy {
                    attempts: 1,
                    per_try_timeout: Duration::from_secs(1),
                    retry_on: vec![serde_json::Value::String("invalid".to_string())],
                }),
                request_headers: None,
                response_headers: None,
                rewrite: None,
                destinations: vec![],
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(err.to_string().contains("unsupported retry condition"));
    }

    #[test]
    fn from_runtime_round_trips_retry_flags() {
        use pavis_core::*;
        let flags = RetryFlags(RETRY_FIVE_XX | RETRY_RESET);
        let values = super::retry_flags_to_values(flags);
        assert!(values.contains(&serde_json::Value::String("5xx".to_string())));
        assert!(values.contains(&serde_json::Value::String("reset".to_string())));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn from_runtime_handles_rewrites() {
        use pavis_core::*;
        let runtime_vhost = VirtualHost {
            host: Host("example.com".to_string()),
            paths: vec![pavis_core::Route {
                matcher: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Disabled,
                retry: RetryPolicy::Disabled,
                request_headers: HeadersPolicy::Disabled,
                response_headers: HeadersPolicy::Disabled,
                rewrite: Rewrite {
                    path: RewritePath::Prefix {
                        from: Path("/".to_string()),
                        to: Path("/api".to_string()),
                    },
                    host: RewriteHost::Literal {
                        host: Hostname("backend".to_string()),
                    },
                },
                destinations: vec![],
            }],
        };

        let serde_vhost = from_runtime(vec![runtime_vhost]);
        let rewrite = serde_vhost[0].paths[0].rewrite.as_ref().unwrap();
        assert_eq!(rewrite.path_prefix_rewrite.as_deref(), Some("/api"));
        assert_eq!(rewrite.host_rewrite_literal.as_deref(), Some("backend"));
    }
}

pub(super) fn to_runtime(routes: Vec<VirtualHost>) -> Result<Vec<pavis_core::VirtualHost>> {
    let mut runtime_routes = Vec::new();

    for v in routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let request_headers = to_runtime_headers(p.request_headers);
            let response_headers = to_runtime_headers(p.response_headers);
            let matcher = p.matcher.unwrap_or_else(default_matcher);

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| {
                    let weight = u16::try_from(d.weight)
                        .map_err(|_| anyhow::anyhow!("destination weight exceeds u16::MAX"))?;
                    let weight = NonZeroU16::new(weight)
                        .ok_or_else(|| anyhow::anyhow!("destination weight must be > 0"))?;
                    Ok(pavis_core::Destination {
                        upstream: pavis_core::UpstreamName(d.upstream),
                        weight: pavis_core::Weight(weight),
                    })
                })
                .collect::<Result<Vec<_>>>()?;

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
                let attempts = NonZeroU16::new(
                    u16::try_from(r.attempts)
                        .map_err(|_| anyhow::anyhow!("retry.attempts exceeds u16::MAX"))?,
                )
                .ok_or_else(|| anyhow::anyhow!("retry.attempts must be > 0"))?;

                let per_try_ms = u32::try_from(r.per_try_timeout.as_millis())
                    .map_err(|_| anyhow::anyhow!("retry.per_try_timeout exceeds u32::MAX ms"))?;
                let per_try = if let Some(ms) = NonZeroU32::new(per_try_ms) {
                    pavis_core::TryTimeout::Enabled(pavis_core::Duration(ms))
                } else {
                    pavis_core::TryTimeout::Disabled
                };

                let on = parse_retry_flags(&r.retry_on)?;
                pavis_core::RetryPolicy::Enabled {
                    attempts,
                    per_try,
                    on,
                }
            } else {
                pavis_core::RetryPolicy::Disabled
            };

            let rewrite = match p.rewrite {
                None => pavis_core::Rewrite {
                    path: pavis_core::RewritePath::Disabled,
                    host: pavis_core::RewriteHost::Disabled,
                },
                Some(r) => {
                    let path = match r.path_prefix_rewrite {
                        Some(to) => {
                            let from = matcher_path(&matcher);
                            pavis_core::RewritePath::Prefix {
                                from: pavis_core::Path(from),
                                to: pavis_core::Path(to),
                            }
                        }
                        None => pavis_core::RewritePath::Disabled,
                    };
                    let host = match r.host_rewrite_literal {
                        Some(host) => pavis_core::RewriteHost::Literal {
                            host: pavis_core::Hostname(host),
                        },
                        None => pavis_core::RewriteHost::Disabled,
                    };
                    pavis_core::Rewrite { path, host }
                }
            };

            let matcher = match matcher {
                Matcher::Prefix { path } => pavis_core::PathMatch::Prefix {
                    path: pavis_core::Path(path),
                },
                Matcher::Exact { path } => pavis_core::PathMatch::Exact {
                    path: pavis_core::Path(path),
                },
                Matcher::Regex { path } => pavis_core::PathMatch::Regex {
                    path: pavis_core::Path(path),
                },
            };

            paths.push(pavis_core::Route {
                matcher,
                timeout,
                retry,
                request_headers,
                response_headers,
                rewrite,
                destinations,
            });
        }

        runtime_routes.push(pavis_core::VirtualHost {
            host: pavis_core::Host(v.host),
            paths,
        });
    }

    Ok(runtime_routes)
}

fn default_matcher() -> Matcher {
    Matcher::Prefix {
        path: "/".to_string(),
    }
}

fn matcher_path(matcher: &Matcher) -> String {
    match matcher {
        Matcher::Prefix { path } => path.clone(),
        Matcher::Exact { path } => path.clone(),
        Matcher::Regex { path } => path.clone(),
    }
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

fn parse_retry_flags(values: &[serde_json::Value]) -> Result<pavis_core::RetryFlags> {
    let mut flags = 0u8;
    for v in values {
        let s = v
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("retry.retry_on entries must be strings"))?;
        match s {
            "5xx" | "five_xx" => flags |= pavis_core::RETRY_FIVE_XX,
            "connect_failure" => flags |= pavis_core::RETRY_CONNECT_FAILURE,
            "reset" => flags |= pavis_core::RETRY_RESET,
            "refused" => flags |= pavis_core::RETRY_REFUSED,
            other => {
                return Err(anyhow::anyhow!("unsupported retry condition: {}", other));
            }
        }
    }
    Ok(pavis_core::RetryFlags(flags))
}

pub(super) fn from_runtime(routes: Vec<pavis_core::VirtualHost>) -> Vec<VirtualHost> {
    let mut serde_routes = Vec::new();

    for v in routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let request_headers = from_runtime_headers(&p.request_headers);
            let response_headers = from_runtime_headers(&p.response_headers);

            let destinations = p
                .destinations
                .into_iter()
                .map(|d| WeightedDestination {
                    upstream: d.upstream.0,
                    weight: d.weight.0.get() as u32,
                })
                .collect();

            let timeout = match p.timeout {
                pavis_core::Timeout::Disabled => None,
                pavis_core::Timeout::Enabled(d) => {
                    Some(std::time::Duration::from_millis(d.0.get() as u64))
                }
            };

            let retry = match p.retry {
                pavis_core::RetryPolicy::Disabled => None,
                pavis_core::RetryPolicy::Enabled {
                    attempts,
                    per_try,
                    on,
                } => {
                    let per_try_timeout = match per_try {
                        pavis_core::TryTimeout::Enabled(d) => {
                            std::time::Duration::from_millis(d.0.get() as u64)
                        }
                        _ => std::time::Duration::from_millis(0),
                    };
                    Some(RetryPolicy {
                        attempts: attempts.get() as usize,
                        per_try_timeout,
                        retry_on: retry_flags_to_values(on),
                    })
                }
            };

            let rewrite = match p.rewrite {
                pavis_core::Rewrite {
                    path: pavis_core::RewritePath::Disabled,
                    host: pavis_core::RewriteHost::Disabled,
                } => None,
                pavis_core::Rewrite { path, host } => Some(RewritePolicy {
                    path_prefix_rewrite: match path {
                        pavis_core::RewritePath::Prefix { to, .. } => Some(to.0),
                        pavis_core::RewritePath::Disabled => None,
                    },
                    host_rewrite_literal: match host {
                        pavis_core::RewriteHost::Literal { host } => Some(host.0),
                        pavis_core::RewriteHost::Disabled => None,
                    },
                }),
            };

            let matcher = match p.matcher {
                pavis_core::PathMatch::Prefix { path } => Matcher::Prefix { path: path.0 },
                pavis_core::PathMatch::Exact { path } => Matcher::Exact { path: path.0 },
                pavis_core::PathMatch::Regex { path } => Matcher::Regex { path: path.0 },
            };

            paths.push(Route {
                matcher: Some(matcher),
                timeout,
                retry,
                request_headers,
                response_headers,
                rewrite,
                destinations,
            });
        }

        serde_routes.push(VirtualHost {
            host: v.host.0,
            paths,
        });
    }

    serde_routes
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
    }
}

pub(crate) fn retry_flags_to_values(flags: pavis_core::RetryFlags) -> Vec<serde_json::Value> {
    let mut values = Vec::new();
    let bits = flags.0;
    if bits & pavis_core::RETRY_FIVE_XX != 0 {
        values.push(serde_json::Value::String("5xx".to_string()));
    }
    if bits & pavis_core::RETRY_CONNECT_FAILURE != 0 {
        values.push(serde_json::Value::String("connect_failure".to_string()));
    }
    if bits & pavis_core::RETRY_RESET != 0 {
        values.push(serde_json::Value::String("reset".to_string()));
    }
    if bits & pavis_core::RETRY_REFUSED != 0 {
        values.push(serde_json::Value::String("refused".to_string()));
    }
    values
}
