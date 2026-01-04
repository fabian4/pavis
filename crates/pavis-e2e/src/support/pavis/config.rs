use anyhow::{Context, Result};
use pavis_codec_serde::config::{
    AccessLogConfig, ConnectionPoolConfig, HeaderAction, HeaderOperations, HttpVersion, Listener,
    LoadBalancer, MatchType, SerdeConfig, TlsConfig, TracingConfig, Upstream, UpstreamTlsConfig,
    VirtualHost, WeightedDestination,
};
use pavis_codec_serde::config::{Endpoint, Route, TelemetryConfig};
use pavis_core::{DiscoveryType, HeaderActionType};
use std::env;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use super::tls::resolve_docker_service_ip;

#[derive(Clone, Copy, Debug)]
pub enum PavisConfigScenario {
    BasicRouting,
    HeaderManipulation,
    HttpVersion,
    RegexMatching,
    ResponseHeaders,
    RoundRobin,
    RouteMatching,
    UnmatchedRoutes,
    UpstreamWeight,
    WeightedSplitting,
    WildcardHost,
}

impl PavisConfigScenario {
    fn name(self) -> &'static str {
        match self {
            Self::BasicRouting => "basic_routing",
            Self::HeaderManipulation => "header_manipulation",
            Self::HttpVersion => "http_version",
            Self::RegexMatching => "regex_matching",
            Self::ResponseHeaders => "response_headers",
            Self::RoundRobin => "round_robin",
            Self::RouteMatching => "route_matching",
            Self::UnmatchedRoutes => "unmatched_routes",
            Self::UpstreamWeight => "upstream_weight",
            Self::WeightedSplitting => "weighted_splitting",
            Self::WildcardHost => "wildcard_host",
        }
    }
}

pub(super) fn generate_config(
    project_root: &Path,
    scenario: PavisConfigScenario,
    mode: &str,
) -> Result<PathBuf> {
    let config_dest = project_root
        .join("crates/pavis-e2e/config")
        .join(format!("generated_{}.yaml", scenario.name()));

    let (backend_v1, backend_v2) = resolve_backend_hosts(mode, project_root)?;
    let config = build_config(scenario, backend_v1, backend_v2);

    write_config(&config_dest, &config)?;
    Ok(config_dest)
}

pub fn write_config(path: &Path, config: &SerdeConfig) -> Result<()> {
    let content = serde_yaml::to_string(config).context("serialize pavis config")?;
    fs::write(path, content)?;
    Ok(())
}

pub fn tls_support_config(
    listen_addr: &str,
    cert_path: &str,
    key_path: &str,
    upstream_host: &str,
    upstream_port: u16,
) -> SerdeConfig {
    SerdeConfig {
        listeners: vec![Listener {
            name: "default".to_string(),
            listen_addr: listen_addr.to_string(),
            worker_threads: None,
            tls: Some(TlsConfig {
                enabled: true,
                cert_path: Some(cert_path.to_string()),
                key_path: Some(key_path.to_string()),
            }),
        }],
        telemetry: TelemetryConfig {
            level: Some("debug".to_string()),
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: AccessLogConfig::Stdout,
            tracing: None,
        },
        upstreams: vec![upstream(
            "backend",
            LoadBalancer::RoundRobin,
            HttpVersion::H1,
            None,
            vec![endpoint(upstream_host, upstream_port, 1)],
        )],
        routes: vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![route(
                MatchType::Prefix,
                "/",
                None,
                None,
                vec![destination("backend", 1)],
            )],
        }],
    }
}

pub fn upstream_tls_config(
    listen_addr: &str,
    upstream_host: &str,
    upstream_port: u16,
) -> SerdeConfig {
    SerdeConfig {
        listeners: vec![Listener {
            name: "default".to_string(),
            listen_addr: listen_addr.to_string(),
            worker_threads: None,
            tls: None,
        }],
        telemetry: TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: AccessLogConfig::Stdout,
            tracing: None,
        },
        upstreams: vec![upstream(
            "backend-tls",
            LoadBalancer::RoundRobin,
            HttpVersion::H1,
            Some(UpstreamTlsConfig {
                enabled: true,
                verify_hostname: Some(false),
                verify_cert: Some(false),
                sni: None,
            }),
            vec![endpoint(upstream_host, upstream_port, 1)],
        )],
        routes: vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![route(
                MatchType::Prefix,
                "/",
                None,
                None,
                vec![destination("backend-tls", 1)],
            )],
        }],
    }
}

fn resolve_backend_hosts(mode: &str, project_root: &Path) -> Result<(String, String)> {
    let backend_v1 = env::var("BACKEND_V1_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let backend_v2 = env::var("BACKEND_V2_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let backend_v1 = resolve_docker_host_if_needed(mode, project_root, backend_v1)?;
    let backend_v2 = resolve_docker_host_if_needed(mode, project_root, backend_v2)?;
    Ok((backend_v1, backend_v2))
}

fn resolve_docker_host_if_needed(mode: &str, project_root: &Path, host: String) -> Result<String> {
    if mode == "docker" && host.parse::<IpAddr>().is_err() {
        resolve_docker_service_ip(project_root, &host)
    } else {
        Ok(host)
    }
}

fn build_config(
    scenario: PavisConfigScenario,
    backend_v1: String,
    backend_v2: String,
) -> SerdeConfig {
    match scenario {
        PavisConfigScenario::BasicRouting => {
            let telemetry = telemetry_with_tracing("pavis-e2e-basic-routing");
            let upstreams = vec![
                upstream(
                    "backend-v1",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v1, 8081, 1)],
                ),
                upstream(
                    "backend-v2",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v2, 8082, 1)],
                ),
            ];
            let headers = header_ops(
                vec![
                    ("x-sidecar-proxy", "pavis-e2e-basic"),
                    ("x-e2e-test", "true"),
                ],
                Vec::new(),
            );
            let routes = vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![route(
                    MatchType::Prefix,
                    "/",
                    Some(headers),
                    None,
                    vec![destination("backend-v1", 50), destination("backend-v2", 50)],
                )],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::HeaderManipulation => {
            let telemetry = telemetry_with_tracing("pavis-e2e-header-manipulation");
            let upstreams = vec![upstream(
                "backend-v1",
                LoadBalancer::RoundRobin,
                HttpVersion::H1,
                None,
                vec![endpoint(&backend_v1, 8081, 1)],
            )];
            let headers = header_ops(
                vec![
                    ("X-Pavis-Added", "Verified"),
                    ("X-Multi-Word", "Hello World"),
                ],
                vec!["X-Pavis-Remove-Me"],
            );
            let routes = vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![route(
                    MatchType::Prefix,
                    "/headers",
                    Some(headers),
                    None,
                    vec![destination("backend-v1", 100)],
                )],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::HttpVersion => {
            let telemetry = telemetry_with_tracing("pavis-e2e-http-version");
            let upstreams = vec![
                upstream(
                    "backend-h1",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v1, 8081, 1)],
                ),
                upstream(
                    "backend-h2",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H2H1,
                    None,
                    vec![endpoint(&backend_v2, 8082, 1)],
                ),
            ];
            let routes = vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![
                    route(
                        MatchType::Prefix,
                        "/h1",
                        None,
                        None,
                        vec![destination("backend-h1", 100)],
                    ),
                    route(
                        MatchType::Prefix,
                        "/h2",
                        None,
                        None,
                        vec![destination("backend-h2", 100)],
                    ),
                ],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::RegexMatching => {
            let telemetry = telemetry_with_tracing("pavis-e2e-regex-matching");
            let upstreams = vec![
                upstream(
                    "backend-v1",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v1, 8081, 1)],
                ),
                upstream(
                    "backend-v2",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v2, 8082, 1)],
                ),
            ];
            let routes = vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![
                    route(
                        MatchType::Regex,
                        "^/api/v[0-9]+/users/[0-9]+$",
                        None,
                        None,
                        vec![destination("backend-v1", 100)],
                    ),
                    route(
                        MatchType::Regex,
                        "^/posts/[a-z0-9-]+$",
                        None,
                        None,
                        vec![destination("backend-v2", 100)],
                    ),
                    route(
                        MatchType::Prefix,
                        "/",
                        None,
                        None,
                        vec![destination("backend-v1", 100)],
                    ),
                ],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::ResponseHeaders => {
            let telemetry = TelemetryConfig {
                level: Some("info".to_string()),
                pingora: Some("warn".to_string()),
                service_name: Some("pavis-e2e-response-headers".to_string()),
                prometheus_addr: None,
                access_log: AccessLogConfig::Stdout,
                tracing: None,
            };
            let upstreams = vec![upstream(
                "echo-backend",
                LoadBalancer::RoundRobin,
                HttpVersion::H1,
                None,
                vec![endpoint(&backend_v1, 8081, 1)],
            )];
            let headers = header_ops(
                vec![
                    ("x-pavis-resp-added", "Verified"),
                    ("x-multi-word-resp", "Hello World"),
                    ("x-proxy-by", "Pavis"),
                ],
                vec!["Server"],
            );
            let routes = vec![VirtualHost {
                host: "response-headers".to_string(),
                paths: vec![route(
                    MatchType::Prefix,
                    "/headers",
                    None,
                    Some(headers),
                    vec![destination("echo-backend", 100)],
                )],
            }];
            base_config("0.0.0.0:8080", None, None, telemetry, upstreams, routes)
        }
        PavisConfigScenario::RoundRobin => {
            let telemetry = telemetry_with_tracing("pavis-e2e-round-robin");
            let upstreams = vec![upstream(
                "backend-mixed",
                LoadBalancer::RoundRobin,
                HttpVersion::H1,
                None,
                vec![
                    endpoint(&backend_v1, 8081, 1),
                    endpoint(&backend_v2, 8082, 1),
                ],
            )];
            let routes = vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![route(
                    MatchType::Prefix,
                    "/round-robin",
                    None,
                    None,
                    vec![destination("backend-mixed", 100)],
                )],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::RouteMatching => {
            let telemetry = telemetry_with_tracing("pavis-e2e-route-matching");
            let upstreams = vec![
                upstream(
                    "backend-v1",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v1, 8081, 1)],
                ),
                upstream(
                    "backend-v2",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v2, 8082, 1)],
                ),
            ];
            let routes = vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![
                    route(
                        MatchType::Exact,
                        "/exact-only",
                        None,
                        None,
                        vec![destination("backend-v1", 100)],
                    ),
                    route(
                        MatchType::Prefix,
                        "/prefix-match",
                        None,
                        None,
                        vec![destination("backend-v2", 100)],
                    ),
                    route(
                        MatchType::Prefix,
                        "/",
                        None,
                        None,
                        vec![destination("backend-v1", 100)],
                    ),
                ],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::UnmatchedRoutes => {
            let telemetry = telemetry_with_tracing("pavis-e2e-unmatched-routes");
            let upstreams = vec![upstream(
                "backend-v1",
                LoadBalancer::RoundRobin,
                HttpVersion::H1,
                None,
                vec![endpoint(&backend_v1, 8081, 1)],
            )];
            let routes = vec![VirtualHost {
                host: "example.com".to_string(),
                paths: vec![route(
                    MatchType::Prefix,
                    "/api",
                    None,
                    None,
                    vec![destination("backend-v1", 100)],
                )],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::UpstreamWeight => {
            let telemetry = telemetry_with_tracing("pavis-e2e-upstream-weight");
            let upstreams = vec![upstream(
                "backend-weighted",
                LoadBalancer::RoundRobin,
                HttpVersion::H1,
                None,
                vec![
                    endpoint(&backend_v1, 8081, 3),
                    endpoint(&backend_v2, 8082, 1),
                ],
            )];
            let routes = vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![route(
                    MatchType::Prefix,
                    "/",
                    None,
                    None,
                    vec![destination("backend-weighted", 100)],
                )],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::WeightedSplitting => {
            let telemetry = telemetry_with_tracing("pavis-e2e-weighted-splitting");
            let upstreams = vec![
                upstream(
                    "backend-v1",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v1, 8081, 1)],
                ),
                upstream(
                    "backend-v2",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v2, 8082, 1)],
                ),
            ];
            let routes = vec![VirtualHost {
                host: "*".to_string(),
                paths: vec![route(
                    MatchType::Prefix,
                    "/weighted-test",
                    None,
                    None,
                    vec![destination("backend-v1", 80), destination("backend-v2", 20)],
                )],
            }];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
        PavisConfigScenario::WildcardHost => {
            let telemetry = telemetry_with_tracing("pavis-e2e-wildcard-host");
            let upstreams = vec![
                upstream(
                    "backend-v1",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v1, 8081, 1)],
                ),
                upstream(
                    "backend-v2",
                    LoadBalancer::RoundRobin,
                    HttpVersion::H1,
                    None,
                    vec![endpoint(&backend_v2, 8082, 1)],
                ),
            ];
            let routes = vec![
                VirtualHost {
                    host: "api.example.com".to_string(),
                    paths: vec![route(
                        MatchType::Prefix,
                        "/",
                        None,
                        None,
                        vec![destination("backend-v1", 100)],
                    )],
                },
                VirtualHost {
                    host: "*".to_string(),
                    paths: vec![route(
                        MatchType::Prefix,
                        "/",
                        None,
                        None,
                        vec![destination("backend-v2", 100)],
                    )],
                },
            ];
            base_config(
                "0.0.0.0:8080",
                Some(2),
                Some(false),
                telemetry,
                upstreams,
                routes,
            )
        }
    }
}

fn base_config(
    listen_addr: &str,
    worker_threads: Option<usize>,
    tls_enabled: Option<bool>,
    telemetry: TelemetryConfig,
    upstreams: Vec<Upstream>,
    routes: Vec<VirtualHost>,
) -> SerdeConfig {
    let tls = tls_enabled.map(|enabled| TlsConfig {
        enabled,
        cert_path: None,
        key_path: None,
    });
    SerdeConfig {
        listeners: vec![Listener {
            name: "default".to_string(),
            listen_addr: listen_addr.to_string(),
            worker_threads,
            tls,
        }],
        telemetry,
        upstreams,
        routes,
    }
}

fn telemetry_with_tracing(service_name: &str) -> TelemetryConfig {
    TelemetryConfig {
        level: Some("info".to_string()),
        pingora: Some("info".to_string()),
        service_name: Some(service_name.to_string()),
        prometheus_addr: Some("0.0.0.0:9091".to_string()),
        access_log: AccessLogConfig::Stdout,
        tracing: Some(TracingConfig {
            enabled: true,
            provider: "opentelemetry".to_string(),
            sampling_rate: 1.0,
        }),
    }
}

fn upstream(
    name: &str,
    load_balancer: LoadBalancer,
    http_version: HttpVersion,
    tls: Option<UpstreamTlsConfig>,
    endpoints: Vec<Endpoint>,
) -> Upstream {
    Upstream {
        name: name.to_string(),
        discovery_type: DiscoveryType::Static,
        load_balancer,
        http_version,
        connection_pool: ConnectionPoolConfig::default(),
        tls,
        circuit_breaker: None,
        health_check: None,
        endpoints,
    }
}

fn endpoint(host: &str, port: u16, weight: u32) -> Endpoint {
    Endpoint {
        address: host.to_string(),
        port,
        weight: Some(weight),
    }
}

fn route(
    match_type: MatchType,
    path: &str,
    request_headers: Option<HeaderOperations>,
    response_headers: Option<HeaderOperations>,
    destinations: Vec<WeightedDestination>,
) -> Route {
    Route {
        match_type,
        path: path.to_string(),
        timeout: None,
        retry: None,
        request_headers,
        response_headers,
        rewrite: None,
        destinations,
    }
}

fn destination(upstream: &str, weight: u32) -> WeightedDestination {
    WeightedDestination {
        upstream: upstream.to_string(),
        weight,
    }
}

fn header_ops(add: Vec<(&str, &str)>, remove: Vec<&str>) -> HeaderOperations {
    let mut actions = Vec::new();
    for (key, value) in add {
        actions.push(HeaderAction {
            key: key.to_string(),
            value: Some(value.to_string()),
            action: HeaderActionType::Set,
        });
    }
    for key in remove {
        actions.push(HeaderAction {
            key: key.to_string(),
            value: None,
            action: HeaderActionType::Remove,
        });
    }
    HeaderOperations { actions }
}
