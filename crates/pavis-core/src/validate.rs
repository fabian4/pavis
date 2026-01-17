mod admin;
mod headers;
mod routes;
mod server;
mod upstreams;

use crate::runtime::{RuntimeConfig, ValidatedRuntimeConfig};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreValidationError {
    #[error("duplicate upstream name: {0}")]
    DuplicateUpstream(String),
    #[error("duplicate listener name: {0}")]
    DuplicateListener(String),
    #[error("duplicate virtual host domain: {0}")]
    DuplicateVirtualHost(String),
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
    #[error("upstream '{0}' has verify=full with sni=disabled")]
    UpstreamTlsSniDisabled(String),
    #[error("upstream '{0}' has verify=full with sni=auto but no DNS endpoints")]
    UpstreamTlsAutoSniRequiresDns(String),
    #[error("upstream '{0}' has invalid health check path")]
    InvalidHealthCheckPath(String),
    #[error("upstream '{0}' health check timeout exceeds interval")]
    HealthCheckTimeoutExceedsInterval(String),
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
    #[error("route '{0}' (host '{1}') has Forward action with no destinations")]
    ForwardHasNoDestinations(String, String),
    #[error("route '{0}' (host '{1}') has rewrite enabled with regex matcher (unsupported)")]
    RewriteRegexConflict(String, String),
    #[error(
        "path '{0}' is not normalized (must start with / and not have trailing slashes unless it is /)"
    )]
    PathNotNormalized(String),
    #[error("regex for route '{route}' is too complex/long")]
    RegexTooLong { route: String },
    #[error("regex cache lock poisoned")]
    RegexCachePoisoned,
}

pub type CoreValidationResult<T> = Result<T, CoreValidationError>;

/// Validate canonical invariants on a fully constructed `RuntimeConfig`.
/// This is intended to be called after parsing/adaptation and before runtime use.
///
/// # Errors
/// Returns `CoreValidationError` if any semantic invariants are violated.
pub fn validate_runtime(config: RuntimeConfig) -> CoreValidationResult<ValidatedRuntimeConfig> {
    use std::collections::HashSet;

    // Validate listener name uniqueness
    let mut listener_names: HashSet<&str> = HashSet::new();
    for listener in &config.listeners {
        if !listener_names.insert(listener.name.0.as_str()) {
            return Err(CoreValidationError::DuplicateListener(
                listener.name.0.clone(),
            ));
        }
        server::validate_server(listener.address, &listener.tls)?;
    }

    // Validate virtual host domain uniqueness
    let mut vhost_domains: HashSet<&str> = HashSet::new();
    for vhost in &config.routes {
        if !vhost_domains.insert(vhost.host.0.as_str()) {
            return Err(CoreValidationError::DuplicateVirtualHost(
                vhost.host.0.clone(),
            ));
        }
    }

    upstreams::validate_upstreams(&config.upstreams)?;
    routes::validate_routes(&config.routes, &config.upstreams)?;
    admin::validate_shutdown(&config.shutdown)?;
    admin::validate_admin(&config.admin)?;
    Ok(ValidatedRuntimeConfig::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        AccessLogPolicy, ActiveHealthCheck, AdminConfig, CircuitBreakerPolicy, ClientAuth,
        ClientCert, ConnectTimeout, ConnectionLimit, Destination, Discovery, Duration, Endpoint,
        EndpointAddr, HeaderName, HeaderValue, Headers, HeadersPolicy, Host, Hostname, HttpVersion,
        IdleTimeout, Listener, ListenerName, LoadBalancer, LogLevel, Metrics,
        OutlierDetectionPolicy, Path, PathMatch, Pool, Port, Principal, RETRY_FIVE_XX, RetryFlags,
        RetryPolicy, Rewrite, RewriteHost, RewritePath, Route, RouteAction, SampleRate,
        ServiceName, ShutdownPolicy, SniName, Telemetry, Timeout, TlsConfig, TlsPolicy, TlsVerify,
        TracingPolicy, TracingProvider, TryTimeout, Upstream, UpstreamCa, UpstreamId, UpstreamName,
        VirtualHost, Weight, WorkerCount,
    };
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::NonZeroU16;
    use std::sync::{Arc, Mutex};

    fn duration_ms(ms: u32) -> Duration {
        Duration(std::num::NonZeroU32::new(ms).unwrap())
    }

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
                    endpoint: "http://localhost:4317".to_string(),
                },
            },
            upstreams: vec![Upstream {
                id: UpstreamId(unsafe { NonZeroU16::new_unchecked(1) }),
                name: UpstreamName("test".to_string()),
                discovery: Discovery::Static,
                balancer: LoadBalancer::RoundRobin,
                protocol: HttpVersion::H1,
                pool: Pool {
                    idle: IdleTimeout::Enabled(duration_ms(60_000)),
                    connect: ConnectTimeout::Enabled(duration_ms(5_000)),
                    max: ConnectionLimit::Unlimited,
                },
                outlier_detection: OutlierDetectionPolicy::Disabled,
                circuit_breaker: CircuitBreakerPolicy::Disabled,
                health_check: ActiveHealthCheck::Disabled,
                tls: TlsPolicy::Enabled {
                    verify: TlsVerify::Full,
                    sni: SniName::Name(Hostname("example.com".to_string())),
                    cert: ClientCert::Disabled,
                    ca: UpstreamCa::System,
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
                    request_headers: Arc::new(HeadersPolicy::Enabled {
                        rules: Headers {
                            set_headers: vec![(
                                HeaderName("x-foo".to_string()),
                                HeaderValue("bar".to_string()),
                            )],
                            append_headers: Vec::new(),
                            add_headers: Vec::new(),
                            remove_headers: vec![HeaderName("x-remove".to_string())],
                        },
                    }),
                    response_headers: Arc::new(HeadersPolicy::Disabled),
                    principal: Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("test".to_string()),
                        weight: Weight(unsafe { NonZeroU16::new_unchecked(1) }),
                    }]),
                }],
            }],
            shutdown: ShutdownPolicy::Disabled,
            admin: AdminConfig::Disabled,
        }
    }

    #[test]
    fn validate_rejects_health_check_path_without_slash() {
        let mut config = base_config();
        config.upstreams[0].health_check = ActiveHealthCheck::Enabled {
            path: Path("healthz".to_string()),
            interval: duration_ms(1000),
            timeout: duration_ms(1000),
        };
        let err = validate_runtime(config).expect_err("expected validation error");
        assert_eq!(
            err,
            CoreValidationError::InvalidHealthCheckPath("test".to_string())
        );
    }

    #[test]
    fn validate_rejects_health_check_timeout_exceeds_interval() {
        let mut config = base_config();
        config.upstreams[0].health_check = ActiveHealthCheck::Enabled {
            path: Path("/healthz".to_string()),
            interval: duration_ms(1000),
            timeout: duration_ms(2000),
        };
        let err = validate_runtime(config).expect_err("expected validation error");
        assert_eq!(
            err,
            CoreValidationError::HealthCheckTimeoutExceedsInterval("test".to_string())
        );
    }

    #[test]
    fn valid_config_passes() {
        let cfg = base_config();
        assert!(validate_runtime(cfg.clone()).is_ok());
    }

    #[test]
    fn upstream_verify_full_rejects_disabled_sni() {
        let mut cfg = base_config();
        if let TlsPolicy::Enabled { sni, .. } = &mut cfg.upstreams[0].tls {
            *sni = SniName::Disabled;
        }
        let err = validate_runtime(cfg).expect_err("expected validation error");
        assert!(matches!(
            err,
            CoreValidationError::UpstreamTlsSniDisabled(_)
        ));
    }

    #[test]
    fn upstream_verify_full_auto_sni_allows_ip_endpoints() {
        let mut cfg = base_config();
        if let TlsPolicy::Enabled { sni, .. } = &mut cfg.upstreams[0].tls {
            *sni = SniName::Auto;
        }
        assert!(validate_runtime(cfg).is_ok());
    }

    #[test]
    fn regex_cache_poisoned_returns_error() {
        let cache = Mutex::new(HashMap::new());
        let _ = std::panic::catch_unwind(|| {
            let _guard = cache.lock().unwrap();
            panic!("poison");
        });

        let err = super::routes::validate_regex_with_cache(&cache, ".*", "*")
            .expect_err("expected regex cache error");
        assert!(matches!(err, CoreValidationError::RegexCachePoisoned));
    }

    #[test]
    fn missing_tls_files_fails() {
        let mut cfg = base_config();
        cfg.listeners[0].tls = TlsConfig::Enabled {
            cert_path: Path("".to_string()),
            key_path: Path("key.pem".to_string()),
            client_auth: ClientAuth::Disabled,
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
            client_auth: ClientAuth::Disabled,
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
        }
        .into();
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
        }
        .into();
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
        }
        .into();
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
        }
        .into();
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
        }
        .into();
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
        }
        .into();
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
        if let RouteAction::Forward(destinations) = &mut cfg.routes[0].paths[0].action {
            destinations[0].upstream = UpstreamName("missing".to_string());
        }
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
    fn duplicate_regex_route_fails() {
        let mut cfg = base_config();
        let mut route = cfg.routes[0].paths[0].clone();
        route.matcher = PathMatch::Regex {
            path: Path("^/api$".to_string()),
        };
        cfg.routes[0].paths = vec![route.clone(), route];
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateRoute { .. }));
    }

    #[test]
    fn prefix_and_exact_same_path_is_allowed() {
        let mut cfg = base_config();
        let mut exact = cfg.routes[0].paths[0].clone();
        exact.matcher = PathMatch::Exact {
            path: Path("/api".to_string()),
        };
        let mut prefix = exact.clone();
        prefix.matcher = PathMatch::Prefix {
            path: Path("/api".to_string()),
        };
        cfg.routes[0].paths = vec![exact, prefix];
        validate_runtime(cfg).expect("prefix/exact allowed");
    }

    #[test]
    fn regex_path_not_normalized_is_allowed() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].matcher = PathMatch::Regex {
            path: Path("api".to_string()),
        };
        validate_runtime(cfg).expect("regex normalization is not enforced");
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

    #[test]
    fn rewrite_regex_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].matcher = PathMatch::Regex {
            path: Path("^/api/.*".to_string()),
        };
        cfg.routes[0].paths[0].rewrite = Rewrite {
            path: RewritePath::Prefix {
                from: Path("/api".to_string()),
                to: Path("/v2".to_string()),
            },
            host: RewriteHost::Disabled,
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::RewriteRegexConflict(..)));
    }

    #[test]
    fn duplicate_listener_name_fails() {
        let mut cfg = base_config();
        let mut duplicate = cfg.listeners[0].clone();
        duplicate.address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8081);
        cfg.listeners.push(duplicate);
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateListener(_)));
    }

    #[test]
    fn duplicate_virtual_host_fails() {
        let mut cfg = base_config();
        let duplicate = cfg.routes[0].clone();
        cfg.routes.push(duplicate);
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::DuplicateVirtualHost(_)));
    }

    #[test]
    fn client_auth_missing_ca_path_fails() {
        let mut cfg = base_config();
        cfg.listeners[0].tls = TlsConfig::Enabled {
            cert_path: Path("cert.pem".to_string()),
            key_path: Path("key.pem".to_string()),
            client_auth: ClientAuth::Required {
                ca_path: Path("".to_string()),
            },
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));

        cfg.listeners[0].tls = TlsConfig::Enabled {
            cert_path: Path("cert.pem".to_string()),
            key_path: Path("key.pem".to_string()),
            client_auth: ClientAuth::Optional {
                ca_path: Path("".to_string()),
            },
        };
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));
    }

    #[test]
    fn forward_empty_destinations_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].action = RouteAction::Forward(Vec::new());
        let err = validate_runtime(cfg.clone()).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::ForwardHasNoDestinations(..)
        ));
    }
}
