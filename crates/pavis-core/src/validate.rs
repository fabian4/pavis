mod headers;
mod routes;
mod server;
mod upstreams;

use crate::runtime::{MatchType, RuntimeConfig, ValidatedRuntimeConfig};

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
    #[error("{context}: header name cannot be empty")]
    EmptyHeaderName { context: String },
    #[error("{context}: invalid header name '{name}'")]
    InvalidHeaderName { context: String, name: String },
    #[error("{context}: invalid header value for '{name}'")]
    InvalidHeaderValue { context: String, name: String },
    #[error("duplicate route '{route}' (match type {match_type:?}) for host '{host}'")]
    DuplicateRoute {
        host: String,
        route: String,
        match_type: MatchType,
    },
    #[error(
        "path '{0}' is not normalized (must start with / and not have trailing slashes unless it is /)"
    )]
    PathNotNormalized(String),
    #[error("regex for route '{route}' is too complex/long")]
    RegexTooLong { route: String },
}

pub type CoreValidationResult<T> = Result<T, CoreValidationError>;

/// Validate canonical invariants on a fully constructed `RuntimeConfig`.
/// This is intended to be called after parsing/adaptation and before runtime use.
///
/// # Errors
/// Returns `CoreValidationError` if any semantic invariants are violated.
pub fn validate_runtime(config: RuntimeConfig) -> CoreValidationResult<ValidatedRuntimeConfig> {
    for listener in &config.listeners {
        server::validate_server(listener.listen_addr, listener.tls.as_ref())?;
    }
    upstreams::validate_upstreams(&config.upstreams)?;
    routes::validate_routes(&config.routes, &config.upstreams)?;
    Ok(ValidatedRuntimeConfig::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        AccessLogConfig, ConnectionPoolConfig, DiscoveryType, Endpoint, EndpointAddress,
        HeaderAction, HeaderActionType, HeaderOperations, HttpVersion, Listener, LoadBalancer,
        MatchType, RetryPolicy, Route, TelemetryConfig, TlsConfig, TracingConfig, Upstream,
        UpstreamTlsConfig, VirtualHost, WeightedDestination,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn base_config() -> RuntimeConfig {
        RuntimeConfig {
            listeners: vec![Listener {
                name: "default".to_string(),
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080),
                worker_threads: None,
                tls: None,
            }],
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
                discovery_type: DiscoveryType::Static,
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
                    address: EndpointAddress::Ip(SocketAddr::new(
                        IpAddr::V4(Ipv4Addr::LOCALHOST),
                        80,
                    )),
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
                        actions: vec![
                            HeaderAction {
                                key: "x-foo".to_string(),
                                value: Some("bar".to_string()),
                                action: HeaderActionType::Set,
                            },
                            HeaderAction {
                                key: "x-remove".to_string(),
                                value: None,
                                action: HeaderActionType::Remove,
                            },
                        ],
                    }),
                    response_headers: None,
                    rewrite: None,
                    destinations: vec![WeightedDestination {
                        upstream: "test".to_string(),
                        weight: 1,
                    }],
                }],
            }],
        }
    }

    #[test]
    fn valid_config_passes() {
        let cfg = base_config();
        assert!(validate_runtime(cfg.clone()).is_ok());
    }

    #[test]
    fn missing_tls_files_fails() {
        let mut cfg = base_config();
        cfg.listeners[0].tls = Some(TlsConfig {
            enabled: true,
            cert_path: None,
            key_path: Some("key.pem".to_string()),
        });
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));
    }

    #[test]
    fn missing_tls_key_fails() {
        let mut cfg = base_config();
        cfg.listeners[0].tls = Some(TlsConfig {
            enabled: true,
            cert_path: Some("cert.pem".to_string()),
            key_path: None,
        });
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));
    }

    #[test]
    fn invalid_regex_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Regex;
        cfg.routes[0].paths[0].path = "[unclosed".to_string();
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidRegex { .. }));
    }

    #[test]
    fn valid_regex_passes() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Regex;
        cfg.routes[0].paths[0].path = "^/items/[0-9]+$".to_string();
        assert!(validate_runtime(cfg.clone()).is_ok());
    }

    #[test]
    fn invalid_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0]
            .request_headers
            .as_mut()
            .unwrap()
            .actions = vec![HeaderAction {
            key: String::new(),
            value: Some("v".to_string()),
            action: HeaderActionType::Set,
        }];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyHeaderName { .. }));
    }

    #[test]
    fn invalid_header_name_non_empty_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0]
            .request_headers
            .as_mut()
            .unwrap()
            .actions = vec![HeaderAction {
            key: "bad header".to_string(),
            value: Some("v".to_string()),
            action: HeaderActionType::Set,
        }];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn invalid_remove_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0]
            .request_headers
            .as_mut()
            .unwrap()
            .actions = vec![HeaderAction {
            key: String::new(),
            value: None,
            action: HeaderActionType::Remove,
        }];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyHeaderName { .. }));
    }

    #[test]
    fn invalid_remove_header_name_non_empty_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0]
            .request_headers
            .as_mut()
            .unwrap()
            .actions = vec![HeaderAction {
            key: "bad header".to_string(),
            value: None,
            action: HeaderActionType::Remove,
        }];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn invalid_header_value_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0]
            .request_headers
            .as_mut()
            .unwrap()
            .actions = vec![HeaderAction {
            key: "x".to_string(),
            value: Some("\u{7f}".to_string()),
            action: HeaderActionType::Set,
        }];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::InvalidHeaderValue { .. }
        ));
    }

    #[test]
    fn invalid_response_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].response_headers = Some(HeaderOperations {
            actions: vec![HeaderAction {
                key: "bad header".to_string(),
                value: Some("v".to_string()),
                action: HeaderActionType::Set,
            }],
        });
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn empty_upstream_name_fails() {
        let mut cfg = base_config();
        cfg.upstreams[0].name = String::new();
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyUpstreamName));
    }

    #[test]
    fn duplicate_upstream_name_fails() {
        let mut cfg = base_config();
        let mut duplicate = cfg.upstreams[0].clone();
        duplicate.endpoints[0].weight = 10;
        cfg.upstreams.push(duplicate);
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateUpstream(_)));
    }

    #[test]
    fn endpoint_weight_zero_fails() {
        let mut cfg = base_config();
        cfg.upstreams[0].endpoints[0].weight = 0;
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::EndpointWeightZero(_)));
    }

    #[test]
    fn unknown_destination_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].destinations[0].upstream = "missing".to_string();
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::UnknownDestination(_, _, _)
        ));
    }

    #[test]
    fn destination_weight_zero_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].destinations[0].weight = 0;
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::DestinationWeightZero(_, _, _)
        ));
    }

    #[test]
    fn path_not_normalized_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].path = "api".to_string();
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::PathNotNormalized(_)));

        cfg.routes[0].paths[0].path = "/api/".to_string();
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::PathNotNormalized(_)));
    }

    #[test]
    fn duplicate_prefix_route_fails() {
        let mut cfg = base_config();
        let mut route = cfg.routes[0].paths[0].clone();
        route.path = "/api".to_string();
        cfg.routes[0].paths = vec![route.clone(), route];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateRoute { .. }));
    }

    #[test]
    fn duplicate_exact_route_fails() {
        let mut cfg = base_config();
        let mut route = cfg.routes[0].paths[0].clone();
        route.match_type = MatchType::Exact;
        cfg.routes[0].paths = vec![route.clone(), route];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateRoute { .. }));
    }

    #[test]
    fn regex_too_long_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Regex;
        cfg.routes[0].paths[0].path = "a".repeat(2049);
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::RegexTooLong { .. }));
    }
}
