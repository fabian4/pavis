use anyhow::Result;
use pavis_core::RouteAction as CoreRouteAction;
use std::num::{NonZeroU16, NonZeroU32};

use crate::config::types::{
    HeaderOperations, Matcher, PrincipalConfig, RetryPolicy, RewritePolicy, Route,
    RouteAction as CodecRouteAction, VirtualHost, WeightedDestination,
};

pub(super) fn to_runtime(routes: Vec<VirtualHost>) -> Result<Vec<pavis_core::VirtualHost>> {
    let mut runtime_routes = Vec::new();

    for v in routes {
        let mut paths = Vec::new();
        for p in v.paths {
            let request_headers = to_runtime_headers(p.request_headers);
            let response_headers = to_runtime_headers(p.response_headers);
            let matcher = p.matcher.unwrap_or_else(default_matcher);

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
                    let path = match r.path {
                        Some(to) => {
                            let from = matcher_path(&matcher);
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

pub(super) fn from_runtime(routes: Vec<pavis_core::VirtualHost>) -> Vec<VirtualHost> {
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
                    panic!("unknown route action variant");
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

            let matcher = match p.matcher {
                pavis_core::PathMatch::Prefix { path } => Matcher::Prefix { path: path.0 },
                pavis_core::PathMatch::Exact { path } => Matcher::Exact { path: path.0 },
                pavis_core::PathMatch::Regex { path } => Matcher::Regex { path: path.0 },
                #[allow(unreachable_patterns)]
                _ => {
                    panic!("unknown path match variant");
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

    serde_routes
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{
        Matcher, RetryPolicy, Route, RouteAction as CodecRouteAction, VirtualHost,
        WeightedDestination,
    };
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
                matcher: Some(Matcher::Prefix {
                    path: "/".to_string(),
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
                matcher: Some(Matcher::Prefix {
                    path: "/".to_string(),
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
                principal: None,
                rewrite: None,
                action: CodecRouteAction::Forward {
                    destinations: vec![],
                },
            }],
        };
        let err = to_runtime(vec![vhost]).unwrap_err();
        assert!(err.to_string().contains("retry.attempts must be > 0"));

        // Test attempts overflow
        let vhost = VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: Some(RetryPolicy {
                    attempts: 70000,
                    per_try_timeout: Duration::from_secs(1),
                    retry_on: vec![],
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
        assert!(err.to_string().contains("retry.attempts exceeds u16::MAX"));
    }

    #[test]
    fn from_runtime_handles_timeouts_and_retries() {
        use pavis_core::*;
        let runtime_vhost = pavis_core::VirtualHost {
            host: Host("example.com".to_string()),
            paths: vec![pavis_core::Route {
                matcher: PathMatch::Prefix {
                    path: Path("/".to_string()),
                },
                timeout: Timeout::Enabled(pavis_core::Duration(NonZeroU32::new(5000).unwrap())),
                retry: RetryPolicy::Enabled {
                    attempts: NonZeroU16::new(3).unwrap(),
                    per_try: TryTimeout::Enabled(pavis_core::Duration(
                        NonZeroU32::new(1000).unwrap(),
                    )),
                    on: RetryFlags(RETRY_FIVE_XX),
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

        let serde_vhost = from_runtime(vec![runtime_vhost]);
        let route = &serde_vhost[0].paths[0];
        assert_eq!(route.timeout, Some(std::time::Duration::from_secs(5)));
        assert_eq!(route.retry.as_ref().unwrap().attempts, 3);
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
            Matcher, PrincipalConfig, Route, RouteAction as CodecRouteAction, VirtualHost,
        };
        use pavis_core::{PathMatch, Principal, RouteAction as CoreRouteAction};

        let vhost = VirtualHost {
            host: "example.com".to_string(),
            paths: vec![
                // 1. Redirect, Authenticated Principal, Exact Match
                Route {
                    matcher: Some(Matcher::Exact {
                        path: "/secure".to_string(),
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
                    matcher: Some(Matcher::Regex {
                        path: "^/admin/.*".to_string(),
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
        match &paths[0].matcher {
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
        match &paths[1].matcher {
            PathMatch::Regex { path } => assert_eq!(path.0, "^/admin/.*"),
            _ => panic!("expected regex match"),
        }

        // Round trip back
        let serde_back = from_runtime(runtime);
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
}
