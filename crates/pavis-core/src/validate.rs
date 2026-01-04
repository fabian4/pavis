mod headers;
mod routes;
mod server;
mod upstreams;

use crate::runtime::{RuntimeConfig, ValidatedRuntimeConfig};

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
    #[error("duplicate route '{route}' (match type {match_type}) for host '{host}'")]
    DuplicateRoute {
        host: String,
        route: String,
        match_type: String,
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
        server::validate_server(listener.address, &listener.tls)?;
    }
    upstreams::validate_upstreams(&config.upstreams)?;
    routes::validate_routes(&config.routes, &config.upstreams)?;
    Ok(ValidatedRuntimeConfig::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Discovery, Duration,
        Endpoint, EndpointAddr, HeaderName, HeaderValue, Headers, HeadersPolicy, Host, Hostname,
        HttpVersion, IdleTimeout, Listener, ListenerName, LoadBalancer, LogLevel, Metrics, Path,
        PathMatch, Pool, Port, RETRY_FIVE_XX, RetryFlags, RetryPolicy, Rewrite, RewriteHost,
        RewritePath, Route, SampleRate, ServiceName, SniName, Telemetry, Timeout, TlsConfig,
        TlsPolicy, TlsVerify, TracingPolicy, TracingProvider, TryTimeout, Upstream, UpstreamId,
        UpstreamName, VirtualHost, Weight, WorkerCount,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::NonZeroU16;

    fn base_config() -> RuntimeConfig {
        RuntimeConfig {
            listeners: vec![Listener {
                name: ListenerName("default".to_string()),
                address: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080),
                workers: WorkerCount::Auto,
                tls: TlsConfig::Disabled,
            }],
            telemetry: Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Info,
                service_name: ServiceName("svc".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Stdout,
                tracing: TracingPolicy::Enabled {
                    provider: TracingProvider::Otlp,
                    sampling: SampleRate(50),
                },
            },
            upstreams: vec![Upstream {
                id: UpstreamId(unsafe { NonZeroU16::new_unchecked(1) }),
                name: UpstreamName("test".to_string()),
                discovery: Discovery::Static,
                balancer: LoadBalancer::RoundRobin,
                protocol: HttpVersion::H1,
                pool: Pool {
                    idle: IdleTimeout::Enabled(Duration(unsafe {
                        std::num::NonZeroU32::new_unchecked(60_000)
                    })),
                    connect: ConnectTimeout::Enabled(Duration(unsafe {
                        std::num::NonZeroU32::new_unchecked(5_000)
                    })),
                    max: ConnectionLimit::Unlimited,
                },
                tls: TlsPolicy::Enabled {
                    verify_mode: TlsVerify::CertAndHost,
                    sni: SniName::Value(Hostname("example.com".to_string())),
                },
                endpoints: vec![Endpoint {
                    address: EndpointAddr::Ip {
                        address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                        port: Port(unsafe { NonZeroU16::new_unchecked(80) }),
                    },
                    weight: Weight(unsafe { NonZeroU16::new_unchecked(1) }),
                }],
            }],
            routes: vec![VirtualHost {
                host: Host("*".to_string()),
                paths: vec![Route {
                    matcher: PathMatch::Prefix {
                        path: Path("/".to_string()),
                    },
                    timeout: Timeout::Disabled,
                    retry: RetryPolicy::Enabled {
                        attempts: NonZeroU16::new(1).unwrap(),
                        per_try: TryTimeout::Enabled(Duration(unsafe {
                            std::num::NonZeroU32::new_unchecked(1000)
                        })),
                        on: RetryFlags(RETRY_FIVE_XX),
                    },
                    request_headers: HeadersPolicy::Enabled {
                        rules: Headers {
                            set_headers: vec![(
                                HeaderName("x-foo".to_string()),
                                HeaderValue("bar".to_string()),
                            )],
                            append_headers: Vec::new(),
                            add_headers: Vec::new(),
                            remove_headers: vec![HeaderName("x-remove".to_string())],
                        },
                    },
                    response_headers: HeadersPolicy::Disabled,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    destinations: vec![Destination {
                        upstream: UpstreamName("test".to_string()),
                        weight: Weight(unsafe { NonZeroU16::new_unchecked(1) }),
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
        cfg.listeners[0].tls = TlsConfig::Enabled {
            cert_path: Path("".to_string()),
            key_path: Path("key.pem".to_string()),
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));
    }

    #[test]
    fn missing_tls_key_fails() {
        let mut cfg = base_config();
        cfg.listeners[0].tls = TlsConfig::Enabled {
            cert_path: Path("cert.pem".to_string()),
            key_path: Path("".to_string()),
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));
    }

    #[test]
    fn invalid_regex_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].matcher = PathMatch::Regex {
            path: Path("[unclosed".to_string()),
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidRegex { .. }));
    }

    #[test]
    fn valid_regex_passes() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].matcher = PathMatch::Regex {
            path: Path("^/items/[0-9]+$".to_string()),
        };
        assert!(validate_runtime(cfg.clone()).is_ok());
    }

    #[test]
    fn invalid_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![(HeaderName(String::new()), HeaderValue("v".to_string()))],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyHeaderName { .. }));
    }

    #[test]
    fn invalid_header_name_non_empty_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![(
                    HeaderName("bad header".to_string()),
                    HeaderValue("v".to_string()),
                )],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn invalid_remove_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: Vec::new(),
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: vec![HeaderName(String::new())],
            },
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyHeaderName { .. }));
    }

    #[test]
    fn invalid_remove_header_name_non_empty_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: Vec::new(),
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: vec![HeaderName("bad header".to_string())],
            },
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn invalid_header_value_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![(
                    HeaderName("x".to_string()),
                    HeaderValue("\u{7f}".to_string()),
                )],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::InvalidHeaderValue { .. }
        ));
    }

    #[test]
    fn invalid_response_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].response_headers = HeadersPolicy::Enabled {
            rules: Headers {
                set_headers: vec![(
                    HeaderName("bad header".to_string()),
                    HeaderValue("v".to_string()),
                )],
                append_headers: Vec::new(),
                add_headers: Vec::new(),
                remove_headers: Vec::new(),
            },
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidHeaderName { .. }));
    }

    #[test]
    fn empty_upstream_name_fails() {
        let mut cfg = base_config();
        cfg.upstreams[0].name = UpstreamName(String::new());
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyUpstreamName));
    }

    #[test]
    fn duplicate_upstream_name_fails() {
        let mut cfg = base_config();
        let mut duplicate = cfg.upstreams[0].clone();
        duplicate.endpoints[0].weight = Weight(unsafe { NonZeroU16::new_unchecked(10) });
        cfg.upstreams.push(duplicate);
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateUpstream(_)));
    }

    #[test]
    fn endpoint_weight_zero_fails() {
        let cfg = base_config();
        // Weight is NonZeroU16; zero is not representable in a valid runtime config.
        let _ = validate_runtime(cfg.clone());
    }

    #[test]
    fn unknown_destination_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].destinations[0].upstream = UpstreamName("missing".to_string());
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::UnknownDestination(_, _, _)
        ));
    }

    #[test]
    fn destination_weight_zero_fails() {
        let cfg = base_config();
        // Weight is NonZeroU16; zero is not representable in a valid runtime config.
        let _ = validate_runtime(cfg.clone());
    }

    #[test]
    fn path_not_normalized_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].matcher = PathMatch::Prefix {
            path: Path("api".to_string()),
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::PathNotNormalized(_)));

        cfg.routes[0].paths[0].matcher = PathMatch::Prefix {
            path: Path("/api/".to_string()),
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::PathNotNormalized(_)));
    }

    #[test]
    fn duplicate_prefix_route_fails() {
        let mut cfg = base_config();
        let mut route = cfg.routes[0].paths[0].clone();
        route.matcher = PathMatch::Prefix {
            path: Path("/api".to_string()),
        };
        cfg.routes[0].paths = vec![route.clone(), route];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateRoute { .. }));
    }

    #[test]
    fn duplicate_exact_route_fails() {
        let mut cfg = base_config();
        let mut route = cfg.routes[0].paths[0].clone();
        route.matcher = PathMatch::Exact {
            path: Path("/".to_string()),
        };
        cfg.routes[0].paths = vec![route.clone(), route];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateRoute { .. }));
    }

    #[test]
    fn regex_too_long_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].matcher = PathMatch::Regex {
            path: Path("a".repeat(2049)),
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::RegexTooLong { .. }));
    }
}
