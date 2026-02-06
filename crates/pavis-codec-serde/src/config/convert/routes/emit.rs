use super::dto_adapter;
use super::materialize::{default_matcher, matcher_path, to_runtime_headers, to_runtime_matcher};
use super::semantic_validate::convert_retry_policy;
use crate::config::types::{
    Matcher, PathMatcher, PrincipalConfig, RouteAction as CodecRouteAction, VirtualHost,
};
use anyhow::Result;
use pavis_core::{
    Duration, RetryPolicy as CoreRetryPolicy, RouteAction as CoreRouteAction, Timeout,
    VirtualHost as CoreVirtualHost,
};
use std::num::{NonZeroU16, NonZeroU32};

pub fn to_runtime(routes: Vec<VirtualHost>) -> Result<Vec<CoreVirtualHost>> {
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
                    Timeout::Enabled(Duration(ms))
                }
                None => Timeout::Disabled,
            };

            let retry = if let Some(r) = p.retry {
                convert_retry_policy(r, &timeout, vh_index, path_index)?
            } else {
                CoreRetryPolicy::Disabled
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
                    pavis_core::Principal::Authenticated {
                        spiffe: pavis_core::SpiffeId(spiffe),
                    }
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

        runtime_routes.push(CoreVirtualHost {
            host: pavis_core::Host(v.host),
            paths,
        });
    }

    Ok(runtime_routes)
}

pub fn from_runtime(routes: Vec<CoreVirtualHost>) -> Result<Vec<VirtualHost>> {
    dto_adapter::from_runtime(routes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{RewritePolicy, Route, WeightedDestination};

    #[test]
    fn test_to_runtime_full() {
        let routes = vec![VirtualHost {
            host: "*".into(),
            paths: vec![
                Route {
                    matcher: Some(Matcher {
                        path: PathMatcher::Prefix { path: "/p".into() },
                        method: Some("GET".into()),
                        methods: None,
                        headers: None,
                    }),
                    timeout: Some(std::time::Duration::from_millis(1000)),
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    rewrite: Some(RewritePolicy {
                        path: Some("/new".into()),
                        host: Some("new.com".into()),
                    }),
                    action: CodecRouteAction::Forward {
                        destinations: vec![WeightedDestination {
                            upstream: "u1".into(),
                            weight: 1,
                        }],
                    },
                    principal: Some(PrincipalConfig::Authenticated {
                        spiffe: "s1".into(),
                    }),
                },
                Route {
                    matcher: Some(Matcher {
                        path: PathMatcher::Exact { path: "/e".into() },
                        method: None,
                        methods: Some(vec!["POST".into()]),
                        headers: None,
                    }),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    rewrite: None,
                    action: CodecRouteAction::Redirect {
                        status: 301,
                        location: "loc".into(),
                    },
                    principal: None,
                },
            ],
        }];

        let res = to_runtime(routes).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].paths.len(), 2);
    }

    #[test]
    fn test_to_runtime_direct() {
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                action: CodecRouteAction::Direct {
                    status: 200,
                    body: "ok".into(),
                },
                principal: Some(PrincipalConfig::Prefix {
                    prefix: "p1".into(),
                }),
            }],
        }];
        let res = to_runtime(routes).unwrap();
        assert!(matches!(
            res[0].paths[0].action,
            CoreRouteAction::Direct { status: 200, .. }
        ));
    }

    #[test]
    fn test_to_runtime_regex() {
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![Route {
                matcher: Some(Matcher {
                    path: PathMatcher::Regex {
                        path: "/r/.*".into(),
                    },
                    method: None,
                    methods: None,
                    headers: None,
                }),
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                action: CodecRouteAction::Direct {
                    status: 200,
                    body: "ok".into(),
                },
                principal: None,
            }],
        }];
        let res = to_runtime(routes).unwrap();
        assert!(matches!(
            res[0].paths[0].matcher.path,
            pavis_core::PathMatch::Regex { .. }
        ));
    }

    #[test]
    fn test_to_runtime_missed_branches() {
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![
                Route {
                    matcher: Some(Matcher {
                        path: PathMatcher::Exact { path: "/e".into() },
                        method: None,
                        methods: None,
                        headers: None,
                    }),
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    rewrite: Some(RewritePolicy {
                        path: None,
                        host: Some("host.com".into()),
                    }),
                    action: CodecRouteAction::Redirect {
                        status: 302,
                        location: "/loc".into(),
                    },
                    principal: Some(PrincipalConfig::Prefix { prefix: "p".into() }),
                },
                Route {
                    matcher: None,
                    timeout: None,
                    retry: None,
                    request_headers: None,
                    response_headers: None,
                    rewrite: Some(RewritePolicy {
                        path: Some("/to".into()),
                        host: None,
                    }),
                    action: CodecRouteAction::Direct {
                        status: 200,
                        body: "".into(),
                    },
                    principal: None,
                },
            ],
        }];
        let res = to_runtime(routes).unwrap();
        assert_eq!(res[0].paths.len(), 2);
        assert!(matches!(
            res[0].paths[0].action,
            CoreRouteAction::Redirect { status: 302, .. }
        ));
        assert!(matches!(
            res[0].paths[0].principal,
            pavis_core::Principal::Prefix { .. }
        ));
        assert!(matches!(
            res[0].paths[0].rewrite.host,
            pavis_core::RewriteHost::Literal { .. }
        ));
        assert!(matches!(
            res[0].paths[1].rewrite.path,
            pavis_core::RewritePath::Prefix { .. }
        ));
    }

    #[test]
    fn test_to_runtime_weight_overflow() {
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                action: CodecRouteAction::Forward {
                    destinations: vec![WeightedDestination {
                        upstream: "u".into(),
                        weight: u32::MAX,
                    }],
                },
                principal: None,
            }],
        }];
        assert!(to_runtime(routes).is_err());
    }

    #[test]
    fn test_to_runtime_weight_zero() {
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                action: CodecRouteAction::Forward {
                    destinations: vec![WeightedDestination {
                        upstream: "u".into(),
                        weight: 0,
                    }],
                },
                principal: None,
            }],
        }];
        assert!(to_runtime(routes).is_err());
    }

    #[test]
    fn test_to_runtime_timeout_errors() {
        // Zero timeout
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![Route {
                matcher: None,
                timeout: Some(std::time::Duration::from_millis(0)),
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                action: CodecRouteAction::Direct {
                    status: 200,
                    body: "".into(),
                },
                principal: None,
            }],
        }];
        assert!(to_runtime(routes).is_err());

        // Overflow timeout
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![Route {
                matcher: None,
                timeout: Some(std::time::Duration::from_secs(u32::MAX as u64 + 1)),
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                action: CodecRouteAction::Direct {
                    status: 200,
                    body: "".into(),
                },
                principal: None,
            }],
        }];
        assert!(to_runtime(routes).is_err());
    }

    #[test]
    fn test_to_runtime_principal_prefix_empty() {
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                action: CodecRouteAction::Direct {
                    status: 200,
                    body: "".into(),
                },
                principal: Some(PrincipalConfig::Prefix { prefix: "".into() }),
            }],
        }];
        assert!(to_runtime(routes).is_err());
    }

    #[test]
    fn test_to_runtime_principal_any() {
        let routes = vec![VirtualHost {
            host: "h".into(),
            paths: vec![Route {
                matcher: None,
                timeout: None,
                retry: None,
                request_headers: None,
                response_headers: None,
                rewrite: None,
                action: CodecRouteAction::Direct {
                    status: 200,
                    body: "".into(),
                },
                principal: Some(PrincipalConfig::Any),
            }],
        }];
        let res = to_runtime(routes).unwrap();
        assert!(matches!(
            res[0].paths[0].principal,
            pavis_core::Principal::Any
        ));
    }
}
