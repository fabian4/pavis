use crate::runtime::{HeaderOperations, MatchType, RuntimeConfig, Upstream, VirtualHost};
use http::header::{HeaderName, HeaderValue};
use regex::Regex;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreValidationError {
    #[error("duplicate upstream name: {0}")]
    DuplicateUpstream(String),
    #[error("empty upstream name")]
    EmptyUpstreamName,
    #[error("upstream {0} has endpoint with weight 0")]
    EndpointWeightZero(String),
    #[error("route '{0}' (host '{1}') references unknown upstream '{2}'")]
    UnknownDestination(String, String, String),
    #[error("route '{0}' (host '{1}') destination '{2}' has weight 0")]
    DestinationWeightZero(String, String, String),
    #[error("tls enabled but cert_path/key_path missing")]
    MissingTlsFiles,
    #[error("invalid regex for route '{route}' (host '{host}'): {error}")]
    InvalidRegex {
        host: String,
        route: String,
        error: String,
    },
    #[error("route path is not normalized (host '{host}', path '{path}')")]
    PathNotNormalized { host: String, path: String },
    #[error("duplicate route (host '{host}', match '{match_type}', path '{path}')")]
    DuplicateRoute {
        host: String,
        match_type: String,
        path: String,
    },
    #[error("regex path too long (host '{host}', length {length}, max {max})")]
    RegexTooLong {
        host: String,
        length: usize,
        max: usize,
    },
    #[error("{context}: header name cannot be empty")]
    EmptyHeaderName { context: String },
    #[error("{context}: invalid header name '{name}'")]
    InvalidHeaderName { context: String, name: String },
    #[error("{context}: invalid header value for '{name}'")]
    InvalidHeaderValue { context: String, name: String },
}

pub type CoreValidationResult<T> = Result<T, CoreValidationError>;

/// Validate canonical invariants on a fully constructed `RuntimeConfig`.
/// This is intended to be called after parsing/adaptation and before runtime use.
pub fn validate_runtime(config: &RuntimeConfig) -> CoreValidationResult<()> {
    validate_server(config.server.listen_addr, config.server.tls.as_ref())?;
    validate_upstreams(&config.upstreams)?;
    validate_routes(&config.routes, &config.upstreams)?;
    Ok(())
}

const MAX_REGEX_LEN: usize = 2048;

fn validate_server(
    _listen_addr: SocketAddr,
    tls: Option<&crate::runtime::TlsConfig>,
) -> CoreValidationResult<()> {
    // SocketAddr is strictly typed, no parsing check needed.
    // If we wanted to ban specific ports, we could do it here.

    if let Some(tls_cfg) = tls
        && tls_cfg.enabled
        && (tls_cfg.cert_path.is_none() || tls_cfg.key_path.is_none())
    {
        return Err(CoreValidationError::MissingTlsFiles);
    }
    Ok(())
}

fn validate_upstreams(upstreams: &[Upstream]) -> CoreValidationResult<()> {
    let mut names = HashSet::new();

    for u in upstreams {
        if u.name.is_empty() {
            return Err(CoreValidationError::EmptyUpstreamName);
        }
        if !names.insert(&u.name) {
            return Err(CoreValidationError::DuplicateUpstream(u.name.clone()));
        }
        for ep in &u.endpoints {
            if ep.weight == 0 {
                return Err(CoreValidationError::EndpointWeightZero(u.name.clone()));
            }
            // IpAddr is strictly typed.
        }
    }
    Ok(())
}

fn validate_routes(routes: &[VirtualHost], upstreams: &[Upstream]) -> CoreValidationResult<()> {
    let upstream_names: HashSet<&str> = upstreams.iter().map(|u| u.name.as_str()).collect();

    for vhost in routes {
        let mut seen_routes: HashSet<(u8, String)> = HashSet::new();
        for route in &vhost.paths {
            let normalized = normalize_path(&route.path);
            if normalized != route.path {
                return Err(CoreValidationError::PathNotNormalized {
                    host: vhost.host.clone(),
                    path: route.path.clone(),
                });
            }

            let match_key = match route.match_type {
                MatchType::Prefix => 0,
                MatchType::Exact => 1,
                MatchType::Regex => 2,
            };
            if !seen_routes.insert((match_key, normalized.to_string())) {
                return Err(CoreValidationError::DuplicateRoute {
                    host: vhost.host.clone(),
                    match_type: format!("{:?}", route.match_type).to_lowercase(),
                    path: route.path.clone(),
                });
            }

            if route.match_type == MatchType::Regex {
                if route.path.len() > MAX_REGEX_LEN {
                    return Err(CoreValidationError::RegexTooLong {
                        host: vhost.host.clone(),
                        length: route.path.len(),
                        max: MAX_REGEX_LEN,
                    });
                }
                let _compiled =
                    Regex::new(&route.path).map_err(|e| CoreValidationError::InvalidRegex {
                        host: vhost.host.clone(),
                        route: route.path.clone(),
                        error: e.to_string(),
                    })?;
            }

            if let Some(headers) = &route.request_headers {
                validate_headers(headers, &format!("Route '{}' request headers", route.path))?;
            }
            if let Some(headers) = &route.response_headers {
                validate_headers(headers, &format!("Route '{}' response headers", route.path))?;
            }

            validate_destinations(route, vhost, &upstream_names)?;
        }
    }
    Ok(())
}

fn validate_destinations(
    route: &crate::runtime::Route,
    vhost: &VirtualHost,
    upstream_names: &HashSet<&str>,
) -> CoreValidationResult<()> {
    for dest in &route.destinations {
        if !upstream_names.contains(dest.upstream.as_str()) {
            return Err(CoreValidationError::UnknownDestination(
                route.path.clone(),
                vhost.host.clone(),
                dest.upstream.clone(),
            ));
        }
        if dest.weight == 0 {
            return Err(CoreValidationError::DestinationWeightZero(
                route.path.clone(),
                vhost.host.clone(),
                dest.upstream.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_headers(headers: &HeaderOperations, context: &str) -> CoreValidationResult<()> {
    for (name, value) in &headers.add {
        if name.is_empty() {
            return Err(CoreValidationError::EmptyHeaderName {
                context: context.to_string(),
            });
        }
        HeaderName::from_str(name).map_err(|_| CoreValidationError::InvalidHeaderName {
            context: context.to_string(),
            name: name.clone(),
        })?;
        HeaderValue::from_str(value).map_err(|_| CoreValidationError::InvalidHeaderValue {
            context: context.to_string(),
            name: name.clone(),
        })?;
    }

    for name in &headers.remove {
        if name.is_empty() {
            return Err(CoreValidationError::EmptyHeaderName {
                context: context.to_string(),
            });
        }
        HeaderName::from_str(name).map_err(|_| CoreValidationError::InvalidHeaderName {
            context: context.to_string(),
            name: name.clone(),
        })?;
    }

    Ok(())
}

fn normalize_path(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    let mut prev_slash = false;
    for ch in path.chars() {
        if ch == '/' {
            if !prev_slash {
                normalized.push(ch);
            }
            prev_slash = true;
        } else {
            normalized.push(ch);
            prev_slash = false;
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        AccessLogConfig, ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType,
        RetryPolicy, Route, ServerConfig, TelemetryConfig, TlsConfig, TracingConfig, Upstream,
        UpstreamTlsConfig, VirtualHost, WeightedDestination,
    };
    use std::net::{IpAddr, Ipv4Addr};

    fn base_config() -> RuntimeConfig {
        RuntimeConfig {
            server: ServerConfig {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080),
                worker_threads: None,
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::Stdout,
                tracing: Some(TracingConfig {
                    enabled: true,
                    provider: "otlp".to_string(),
                    sampling_rate: 0.5,
                }),
            },
            upstreams: vec![Upstream {
                name: "test".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H1,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 60,
                    connection_timeout_secs: 5,
                },
                tls: Some(UpstreamTlsConfig {
                    enabled: true,
                    verify_hostname: true,
                    verify_cert: true,
                    sni: Some("example.com".to_string()),
                }),
                endpoints: vec![Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: 80,
                    weight: 1,
                }],
            }],
            routes: vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Prefix,
                    path: "/".to_string(),
                    timeout_ms: None,
                    retry_policy: Some(RetryPolicy {
                        attempts: 1,
                        per_try_timeout_ms: 1000,
                        retry_on: vec!["5xx".to_string()],
                    }),
                    request_headers: Some(HeaderOperations {
                        add: vec![("x-foo".to_string(), "bar".to_string())],
                        remove: vec!["x-remove".to_string()],
                    }),
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "test".to_string(),
                        weight: 1,
                    }],
                    compiled_regex: None,
                }],
            }],
        }
    }

    #[test]
    fn valid_config_passes() {
        let cfg = base_config();
        assert!(validate_runtime(&cfg).is_ok());
    }

    #[test]
    fn missing_tls_files_fails() {
        let mut cfg = base_config();
        cfg.server.tls = Some(TlsConfig {
            enabled: true,
            cert_path: None,
            key_path: Some("key.pem".to_string()),
        });
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));
    }

    #[test]
    fn missing_tls_key_fails() {
        let mut cfg = base_config();
        cfg.server.tls = Some(TlsConfig {
            enabled: true,
            cert_path: Some("cert.pem".to_string()),
            key_path: None,
        });
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));
    }

    #[test]
    fn invalid_regex_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Regex;
        cfg.routes[0].paths[0].path = "[unclosed".to_string();
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidRegex { .. }));
    }

    #[test]
    fn valid_regex_passes() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Regex;
        cfg.routes[0].paths[0].path = "^/items/[0-9]+$".to_string();
        assert!(validate_runtime(&cfg).is_ok());
    }

    #[test]
    fn path_not_normalized_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].path = "//api".to_string();
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::PathNotNormalized { .. }));
    }

    #[test]
    fn duplicate_exact_route_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Exact;
        let duplicate = cfg.routes[0].paths[0].clone();
        cfg.routes[0].paths.push(duplicate);
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateRoute { .. }));
    }

    #[test]
    fn duplicate_prefix_route_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Prefix;
        let duplicate = cfg.routes[0].paths[0].clone();
        cfg.routes[0].paths.push(duplicate);
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateRoute { .. }));
    }

    #[test]
    fn regex_too_long_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Regex;
        cfg.routes[0].paths[0].path = "a".repeat(MAX_REGEX_LEN + 1);
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::RegexTooLong { .. }));
    }

    #[test]
    fn invalid_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers.as_mut().unwrap().add =
            vec![("".to_string(), "v".to_string())];
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyHeaderName { .. }));
    }

    #[test]
    fn invalid_header_name_non_empty_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers.as_mut().unwrap().add =
            vec![("bad header".to_string(), "v".to_string())];
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn invalid_remove_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0]
            .request_headers
            .as_mut()
            .unwrap()
            .remove = vec!["".to_string()];
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyHeaderName { .. }));
    }

    #[test]
    fn invalid_remove_header_name_non_empty_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0]
            .request_headers
            .as_mut()
            .unwrap()
            .remove = vec!["bad header".to_string()];
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn invalid_header_value_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers.as_mut().unwrap().add =
            vec![("x".to_string(), "\u{7f}".to_string())];
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::InvalidHeaderValue { .. }
        ));
    }

    #[test]
    fn invalid_response_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].response_headers = Some(HeaderOperations {
            add: vec![("bad header".to_string(), "v".to_string())],
            remove: Vec::new(),
        });
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn empty_upstream_name_fails() {
        let mut cfg = base_config();
        cfg.upstreams[0].name = "".to_string();
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyUpstreamName));
    }

    #[test]
    fn duplicate_upstream_name_fails() {
        let mut cfg = base_config();
        let mut duplicate = cfg.upstreams[0].clone();
        duplicate.endpoints[0].port = 81;
        cfg.upstreams.push(duplicate);
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateUpstream(_)));
    }

    #[test]
    fn endpoint_weight_zero_fails() {
        let mut cfg = base_config();
        cfg.upstreams[0].endpoints[0].weight = 0;
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::EndpointWeightZero(_)));
    }

    #[test]
    fn unknown_destination_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].destinations[0].upstream = "missing".to_string();
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::UnknownDestination(_, _, _)
        ));
    }

    #[test]
    fn destination_weight_zero_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].destinations[0].weight = 0;
        let err = validate_runtime(&cfg).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::DestinationWeightZero(_, _, _)
        ));
    }
}
