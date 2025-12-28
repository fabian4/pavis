use crate::runtime::{HeaderOperations, MatchType, RuntimeConfig, Upstream, VirtualHost};
use http::header::{HeaderName, HeaderValue};
use regex::Regex;
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreValidationError {
    #[error("duplicate upstream name: {0}")]
    DuplicateUpstream(String),
    #[error("empty upstream name")]
    EmptyUpstreamName,
    #[error("upstream {0} has endpoint with weight 0")]
    EndpointWeightZero(String),
    #[error("upstream {0} has endpoint with empty ip/hostname")]
    EmptyEndpointAddress(String),
    #[error("upstream {upstream} has invalid endpoint ip/hostname: {value}")]
    InvalidEndpointAddress { upstream: String, value: String },
    #[error("route '{0}' (host '{1}') references unknown upstream '{2}'")]
    UnknownDestination(String, String, String),
    #[error("route '{0}' (host '{1}') destination '{2}' has weight 0")]
    DestinationWeightZero(String, String, String),
    #[error("invalid listen_addr '{0}'")]
    InvalidListenAddr(String),
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
}

pub type CoreValidationResult<T> = Result<T, CoreValidationError>;

/// Validate canonical invariants on a fully constructed `RuntimeConfig`.
/// This is intended to be called after parsing/adaptation and before runtime use.
pub fn validate_runtime_config(config: &RuntimeConfig) -> CoreValidationResult<()> {
    validate_server(&config.server.listen_addr, config.server.tls.as_ref())?;
    validate_upstreams(&config.upstreams)?;
    validate_routes(&config.routes, &config.upstreams)?;
    Ok(())
}

/// Backward-compatible alias.
pub fn validate_runtime(config: &RuntimeConfig) -> CoreValidationResult<()> {
    validate_runtime_config(config)
}

fn validate_server(
    listen_addr: &str,
    tls: Option<&crate::runtime::TlsConfig>,
) -> CoreValidationResult<()> {
    listen_addr
        .parse::<SocketAddr>()
        .map_err(|_| CoreValidationError::InvalidListenAddr(listen_addr.to_string()))?;

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
    let hostname_regex = Regex::new(
        r"^([a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*)$",
    )
    .expect("hostname regex should compile");

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
            if ep.ip.is_empty() {
                return Err(CoreValidationError::EmptyEndpointAddress(u.name.clone()));
            }
            if IpAddr::from_str(&ep.ip).is_err() && !hostname_regex.is_match(&ep.ip) {
                return Err(CoreValidationError::InvalidEndpointAddress {
                    upstream: u.name.clone(),
                    value: ep.ip.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_routes(routes: &[VirtualHost], upstreams: &[Upstream]) -> CoreValidationResult<()> {
    let upstream_names: HashSet<&str> = upstreams.iter().map(|u| u.name.as_str()).collect();

    for vhost in routes {
        for route in &vhost.paths {
            if route.match_type == MatchType::Regex {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        AccessLogConfig, ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType,
        RetryPolicy, Route, ServerConfig, TelemetryConfig, TlsConfig, TracingConfig, Upstream,
        UpstreamTlsConfig, VirtualHost, WeightedDestination,
    };

    fn base_config() -> RuntimeConfig {
        RuntimeConfig {
            server: ServerConfig {
                listen_addr: "0.0.0.0:8080".to_string(),
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
                    ip: "127.0.0.1".to_string(),
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
        assert!(validate_runtime_config(&cfg).is_ok());
    }

    #[test]
    fn invalid_listen_addr_fails() {
        let mut cfg = base_config();
        cfg.server.listen_addr = "bad".to_string();
        let err = validate_runtime_config(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidListenAddr(_)));
    }

    #[test]
    fn missing_tls_files_fails() {
        let mut cfg = base_config();
        cfg.server.tls = Some(TlsConfig {
            enabled: true,
            cert_path: None,
            key_path: Some("key.pem".to_string()),
        });
        let err = validate_runtime_config(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::MissingTlsFiles));
    }

    #[test]
    fn invalid_endpoint_address_fails() {
        let mut cfg = base_config();
        cfg.upstreams[0].endpoints[0].ip = "bad ip".to_string();
        let err = validate_runtime_config(&cfg).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::InvalidEndpointAddress { .. }
        ));
    }

    #[test]
    fn invalid_regex_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].match_type = MatchType::Regex;
        cfg.routes[0].paths[0].path = "[unclosed".to_string();
        let err = validate_runtime_config(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::InvalidRegex { .. }));
    }

    #[test]
    fn invalid_header_name_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers.as_mut().unwrap().add =
            vec![("".to_string(), "v".to_string())];
        let err = validate_runtime_config(&cfg).unwrap_err();
        assert!(matches!(err, CoreValidationError::EmptyHeaderName { .. }));
    }

    #[test]
    fn invalid_header_value_fails() {
        let mut cfg = base_config();
        cfg.routes[0].paths[0].request_headers.as_mut().unwrap().add =
            vec![("x".to_string(), "\u{7f}".to_string())];
        let err = validate_runtime_config(&cfg).unwrap_err();
        assert!(matches!(
            err,
            CoreValidationError::InvalidHeaderValue { .. }
        ));
    }
}
