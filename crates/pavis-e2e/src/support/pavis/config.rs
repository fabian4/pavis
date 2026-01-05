use anyhow::{Context, Result};
use pavis_codec_serde::config::{
    AccessLogPolicy, ConnectionPoolConfig, Endpoint, HeaderOperations, HttpVersion, Listener,
    LoadBalancer, Matcher, Route, SerdeConfig, TelemetryConfig, TlsConfig, TracingConfig, Upstream,
    UpstreamTlsConfig, VirtualHost, WeightedDestination,
};
use pavis_core::Discovery;
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
            Self::HttpVersion => "http",
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
        listeners: Some(vec![Listener {
            name: "default".to_string(),
            address: listen_addr.to_string(),
            workers: None,
            tls: Some(TlsConfig {
                cert_path: Some(cert_path.to_string()),
                key_path: Some(key_path.to_string()),
            }),
        }]),
        telemetry: Some(TelemetryConfig {
            level: Some("debug".to_string()),
            pingora: None,
            service_name: None,
            metrics: None,
            access_log: Some(AccessLogPolicy::Stdout),
            tracing: None,
        }),
        upstreams: Some(vec![upstream(
            "backend",
            LoadBalancer::RoundRobin,
            HttpVersion::H1,
            None,
            vec![endpoint(upstream_host, upstream_port, 1)],
        )]),
        routes: Some(vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![route(
                Matcher::Prefix {
                    path: "/".to_string(),
                },
                None,
                None,
                vec![destination("backend", 1)],
            )],
        }]),
    }
}

pub fn upstream_tls_config(
    listen_addr: &str,
    upstream_host: &str,
    upstream_port: u16,
) -> SerdeConfig {
    SerdeConfig {
        listeners: Some(vec![Listener {
            name: "default".to_string(),
            address: listen_addr.to_string(),
            workers: None,
            tls: None,
        }]),
        telemetry: Some(TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            metrics: None,
            access_log: Some(AccessLogPolicy::Stdout),
            tracing: None,
        }),
        upstreams: Some(vec![upstream(
            "backend-tls",
            LoadBalancer::RoundRobin,
            HttpVersion::H1,
            Some(UpstreamTlsConfig {
                enabled: Some(true),
                verify_hostname: Some(false),
                verify_cert: Some(false),
                sni: None,
            }),
            vec![endpoint(upstream_host, upstream_port, 1)],
        )]),
        routes: Some(vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![route(
                Matcher::Prefix {
                    path: "/".to_string(),
                },
                None,
                None,
                vec![destination("backend-tls", 1)],
            )],
        }]),
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
                    Matcher::Prefix {
                        path: "/".to_string(),
                    },
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
                    Matcher::Prefix {
                        path: "/headers".to_string(),
                    },
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
                        Matcher::Prefix {
                            path: "/h1".to_string(),
                        },
                        None,
                        None,
                        vec![destination("backend-h1", 100)],
                    ),
                    route(
                        Matcher::Prefix {
                            path: "/h2".to_string(),
                        },
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
                        Matcher::Regex {
                            path: "^/api/v[0-9]+/users/[0-9]+$".to_string(),
                        },
                        None,
                        None,
                        vec![destination("backend-v1", 100)],
                    ),
                    route(
                        Matcher::Regex {
                            path: "^/posts/[a-z0-9-]+$".to_string(),
                        },
                        None,
                        None,
                        vec![destination("backend-v2", 100)],
                    ),
                    route(
                        Matcher::Prefix {
                            path: "/".to_string(),
                        },
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
                metrics: None,
                access_log: Some(AccessLogPolicy::Stdout),
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
                    Matcher::Prefix {
                        path: "/headers".to_string(),
                    },
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
                    Matcher::Prefix {
                        path: "/round-robin".to_string(),
                    },
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
                        Matcher::Exact {
                            path: "/exact-only".to_string(),
                        },
                        None,
                        None,
                        vec![destination("backend-v1", 100)],
                    ),
                    route(
                        Matcher::Prefix {
                            path: "/prefix-match".to_string(),
                        },
                        None,
                        None,
                        vec![destination("backend-v2", 100)],
                    ),
                    route(
                        Matcher::Prefix {
                            path: "/".to_string(),
                        },
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
                    Matcher::Prefix {
                        path: "/api".to_string(),
                    },
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
                    Matcher::Prefix {
                        path: "/".to_string(),
                    },
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
                    Matcher::Prefix {
                        path: "/weighted-test".to_string(),
                    },
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
                        Matcher::Prefix {
                            path: "/".to_string(),
                        },
                        None,
                        None,
                        vec![destination("backend-v1", 100)],
                    )],
                },
                VirtualHost {
                    host: "*".to_string(),
                    paths: vec![route(
                        Matcher::Prefix {
                            path: "/".to_string(),
                        },
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
    worker_threads: Option<u16>,
    tls_enabled: Option<bool>,
    telemetry: TelemetryConfig,
    upstreams: Vec<Upstream>,
    routes: Vec<VirtualHost>,
) -> SerdeConfig {
    let tls = tls_enabled.and_then(|enabled| {
        if enabled {
            Some(TlsConfig {
                cert_path: None,
                key_path: None,
            })
        } else {
            None
        }
    });
    SerdeConfig {
        listeners: Some(vec![Listener {
            name: "default".to_string(),
            address: listen_addr.to_string(),
            workers: worker_threads,
            tls,
        }]),
        telemetry: Some(telemetry),
        upstreams: Some(upstreams),
        routes: Some(routes),
    }
}

fn telemetry_with_tracing(service_name: &str) -> TelemetryConfig {
    TelemetryConfig {
        level: Some("info".to_string()),
        pingora: Some("info".to_string()),
        service_name: Some(service_name.to_string()),
        metrics: Some("0.0.0.0:9091".to_string()),
        access_log: Some(AccessLogPolicy::Stdout),
        tracing: Some(TracingConfig {
            provider: Some("otlp".to_string()),
            sampling: Some(1),
        }),
    }
}

fn upstream(
    name: &str,
    lb: LoadBalancer,
    http: HttpVersion,
    tls: Option<UpstreamTlsConfig>,
    endpoints: Vec<Endpoint>,
) -> Upstream {
    Upstream {
        id: None,
        name: name.to_string(),
        discovery: Some(Discovery::Static),
        balancer: Some(lb),
        protocol: Some(http),
        pool: Some(ConnectionPoolConfig::default()),
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
    matcher: Matcher,
    request_headers: Option<HeaderOperations>,
    response_headers: Option<HeaderOperations>,
    destinations: Vec<WeightedDestination>,
) -> Route {
    Route {
        matcher: Some(matcher),
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
    HeaderOperations {
        set_headers: add
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        append_headers: Vec::new(),
        add_headers: Vec::new(),
        remove_headers: remove.into_iter().map(|key| key.to_string()).collect(),
    }
}
