mod admin;
mod routes;
mod server;
mod telemetry;
mod upstreams;

use anyhow::Result;

use super::types::{SerdeConfig, StructurallyConfig};

/// Shape-only completion: fills empty containers and optional sub-objects.
/// Does not apply core semantic validation.
pub fn structural(src: SerdeConfig) -> StructurallyConfig {
    StructurallyConfig {
        listeners: src.listeners.unwrap_or_default(),
        telemetry: src.telemetry.unwrap_or_default(),
        upstreams: src.upstreams.unwrap_or_default(),
        routes: src.routes.unwrap_or_default(),
        shutdown: src.shutdown.unwrap_or_default(),
        admin: src.admin.unwrap_or_default(),
    }
}

impl TryFrom<StructurallyConfig> for pavis_core::RuntimeConfig {
    type Error = anyhow::Error;

    fn try_from(src: StructurallyConfig) -> Result<Self, Self::Error> {
        let mut listeners = Vec::with_capacity(src.listeners.len());
        for l in src.listeners {
            listeners.push(server::to_runtime(l)?);
        }

        let telemetry = telemetry::to_runtime(src.telemetry)?;
        let upstreams = upstreams::to_runtime(src.upstreams)?;
        let routes = routes::to_runtime(src.routes)?;
        let shutdown = admin::shutdown_to_runtime(src.shutdown)?;
        let admin = admin::admin_to_runtime(src.admin)?;

        let mut builder = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(telemetry)
            .shutdown(shutdown)
            .admin(admin);
        for listener in listeners {
            builder = builder.add_listener(listener);
        }
        for upstream in upstreams {
            builder = builder.add_upstream(upstream);
        }
        for route in routes {
            builder = builder.add_route(route);
        }
        builder.build().map_err(|err| anyhow::anyhow!(err))
    }
}

impl TryFrom<pavis_core::RuntimeConfig> for SerdeConfig {
    type Error = anyhow::Error;

    fn try_from(binary: pavis_core::RuntimeConfig) -> Result<Self, Self::Error> {
        let listeners = binary
            .listeners
            .into_iter()
            .map(server::from_runtime)
            .collect();
        Ok(SerdeConfig {
            listeners: Some(listeners),
            telemetry: Some(telemetry::from_runtime(binary.telemetry)),
            upstreams: Some(upstreams::from_runtime(binary.upstreams)?),
            routes: Some(routes::from_runtime(binary.routes)?),
            shutdown: Some(admin::shutdown_from_runtime(binary.shutdown)),
            admin: Some(admin::admin_from_runtime(binary.admin)),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::SerdeFormat;
    use crate::config::types::SerdeConfig;
    use pavis_core::{
        AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Duration, Endpoint,
        EndpointAddr, HeaderPredicates, Host, HttpVersion, IdleTimeout, ListenerName, LoadBalancer,
        LogLevel, MethodPredicate, Metrics, Path, PathMatch, Pool, Port, RETRY_CONNECT_FAILURE,
        RETRY_FIVE_XX, RetryFlags, RetryPolicy, Rewrite, RewriteHost, RewritePath, RouteAction,
        RouteMatcher, ServiceName, Telemetry, Timeout, TlsConfig, TlsPolicy, TlsVerify,
        TracingPolicy, TracingProvider, TryTimeout, UpstreamId, UpstreamName, VirtualHost, Weight,
        WorkerCount,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::{NonZeroU16, NonZeroU32};
    use std::time::Duration as StdDuration;

    #[test]
    fn yaml_to_runtime_converts_defaults_and_structures() {
        let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    tls:
      enabled: true
      sni: "backend.local"
    endpoints:
      - address: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        timeout: "1s"
        request_headers:
          set_headers:
            - ["x-added", "1"]
        response_headers:
          remove_headers: ["x-remove"]
        retry:
          attempts: 2
          per_try_timeout: "250ms"
          retry_on: ["5xx", "connect_failure"]
        destinations:
          - upstream: "backend"
            weight: 1
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect("parse yaml");
        let runtime = config.build().expect("convert to runtime");

        let upstream = &runtime.upstreams[0];
        assert_eq!(upstream.endpoints[0].weight.0.get(), 1);
        match upstream.pool.idle {
            IdleTimeout::Enabled(d) => assert_eq!(d.0.get(), 60_000),
            IdleTimeout::Disabled => panic!("idle timeout not populated"),
            _ => panic!("unknown idle timeout"),
        }
        match upstream.pool.connect {
            ConnectTimeout::Enabled(d) => assert_eq!(d.0.get(), 5_000),
            ConnectTimeout::Disabled => panic!("connect timeout not populated"),
            _ => panic!("unknown connect timeout"),
        }
        match upstream.tls {
            TlsPolicy::Enabled { verify, .. } => {
                assert_eq!(verify, TlsVerify::Full);
            }
            TlsPolicy::Disabled => panic!("tls not enabled"),
            _ => panic!("unknown tls policy"),
        }

        let route = &runtime.routes[0].paths[0];
        match route.timeout {
            Timeout::Enabled(d) => assert_eq!(d.0.get(), 1000),
            Timeout::Disabled => panic!("route timeout not populated"),
            _ => panic!("unknown route timeout"),
        }
        match &route.retry {
            RetryPolicy::Enabled {
                attempts,
                per_try,
                on,
            } => {
                assert_eq!(attempts.get(), 2);
                match per_try {
                    TryTimeout::Enabled(d) => assert_eq!(d.0.get(), 250),
                    _ => panic!("per_try timeout not populated"),
                }
                assert_eq!(on.0 & RETRY_FIVE_XX, RETRY_FIVE_XX);
                assert_eq!(on.0 & RETRY_CONNECT_FAILURE, RETRY_CONNECT_FAILURE);
            }
            RetryPolicy::Disabled => panic!("retry policy not enabled"),
            &_ => panic!("unknown retry policy"),
        }
        match &*route.request_headers {
            pavis_core::HeadersPolicy::Enabled { rules } => {
                assert_eq!(rules.set_headers.len(), 1);
                assert_eq!(rules.set_headers[0].0.0, "x-added");
                assert_eq!(rules.set_headers[0].1.0, "1");
            }
            _ => panic!("request headers not enabled"),
        }
        match &*route.response_headers {
            pavis_core::HeadersPolicy::Enabled { rules } => {
                assert_eq!(rules.remove_headers.len(), 1);
                assert_eq!(rules.remove_headers[0].0, "x-remove");
            }
            _ => panic!("response headers not enabled"),
        }
    }

    #[test]
    fn runtime_to_yaml_preserves_values() {
        let listener = pavis_core::ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                8080,
            ))
            .workers(WorkerCount::Count(NonZeroU16::new(2).unwrap()))
            .tls(TlsConfig::Disabled)
            .build()
            .expect("listener");

        let upstream = pavis_core::UpstreamBuilder::new()
            .id(UpstreamId(NonZeroU16::new(7).unwrap()))
            .name(UpstreamName("backend".to_string()))
            .discovery(pavis_core::Discovery::Static)
            .balancer(LoadBalancer::RoundRobin)
            .protocol(HttpVersion::H2)
            .pool(Pool {
                idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(10_000).unwrap())),
                connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(2_000).unwrap())),
                max: ConnectionLimit(NonZeroU32::new(10).unwrap()),
                ..Pool::default()
            })
            .tls(TlsPolicy::Enabled {
                verify: TlsVerify::CaOnly,
                sni: pavis_core::SniName::Name(pavis_core::Hostname("backend.local".to_string())),
                cert: pavis_core::ClientCert::Disabled,
                ca: pavis_core::UpstreamCa::System,
            })
            .add_endpoint(Endpoint {
                address: EndpointAddr::Ip {
                    address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: Port(NonZeroU16::new(8081).unwrap()),
                },
                weight: Weight(NonZeroU16::new(3).unwrap()),
            })
            .build()
            .expect("upstream");

        let runtime = pavis_core::RuntimeConfigBuilder::new()
            .telemetry(Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Warn,
                service_name: ServiceName("svc".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Enabled {
                    provider: TracingProvider::Otlp,
                    sampling: pavis_core::SampleRate(10),
                    endpoint: "http://localhost:4317".to_string(),
                },
            })
            .shutdown(pavis_core::ShutdownPolicy::Disabled)
            .admin(pavis_core::AdminConfig::Disabled)
            .add_listener(listener)
            .add_upstream(upstream)
            .add_route(VirtualHost {
                host: Host("example.com".to_string()),
                paths: vec![pavis_core::Route {
                    matcher: RouteMatcher {
                        path: PathMatch::Exact {
                            path: Path("/".to_string()),
                        },
                        method: MethodPredicate::Any,
                        headers: HeaderPredicates::None,
                    },
                    timeout: Timeout::Enabled(Duration(NonZeroU32::new(1500).unwrap())),
                    retry: RetryPolicy::Enabled {
                        attempts: NonZeroU16::new(3).unwrap(),
                        per_try: TryTimeout::Enabled(Duration(NonZeroU32::new(500).unwrap())),
                        on: RetryFlags(RETRY_FIVE_XX),
                    },
                    request_headers: pavis_core::HeadersPolicy::Disabled.into(),
                    response_headers: pavis_core::HeadersPolicy::Disabled.into(),
                    principal: pavis_core::Principal::Any,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    action: RouteAction::Forward(vec![Destination {
                        upstream: UpstreamName("backend".to_string()),
                        weight: Weight(NonZeroU16::new(2).unwrap()),
                    }]),
                }],
            })
            .build()
            .expect("runtime");

        let config = SerdeConfig::try_from(runtime).expect("serde config");
        let listeners = config.listeners.as_ref().expect("listeners");
        let telemetry = config.telemetry.as_ref().expect("telemetry");
        let upstreams = config.upstreams.as_ref().expect("upstreams");
        assert_eq!(listeners[0].address, "127.0.0.1:8080");
        assert_eq!(listeners[0].workers, Some(2));
        assert_eq!(telemetry.level, Some("info".to_string()));
        assert_eq!(telemetry.access_log, Some(AccessLogPolicy::Disabled));
        let upstream = &upstreams[0];
        assert_eq!(upstream.balancer, Some(LoadBalancer::RoundRobin));
        assert_eq!(upstream.protocol, Some(HttpVersion::H2));
        let pool = upstream.pool.as_ref().expect("pool");
        assert_eq!(pool.idle, Some(StdDuration::from_secs(10)));
        assert_eq!(pool.connect, Some(StdDuration::from_secs(2)));
        let tls = upstream.tls.as_ref().expect("tls config");
        assert_eq!(tls.enabled, Some(true));
        assert_eq!(tls.verify_hostname, Some(false));
        assert_eq!(tls.verify_cert, Some(true));
        assert_eq!(tls.sni.as_deref(), Some("backend.local"));
        assert_eq!(upstream.endpoints[0].weight, Some(3));
        assert_eq!(upstream.endpoints[0].address, "127.0.0.1");
    }

    #[test]
    fn yaml_to_runtime_rejects_invalid_listen_addr() {
        let yaml = r#"
listeners:
  - name: "default"
    address: "invalid-addr"
telemetry: {}
upstreams: []
routes: []
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect("parse yaml");
        let err = config.build().expect_err("invalid listen addr");
        assert!(err.to_string().contains("Invalid address"));
    }

    #[test]
    fn yaml_to_runtime_rejects_invalid_endpoint_ip() {
        let yaml = r#"
listeners:
  - name: "default"
    address: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - address: "not-an-ip"
        port: 8081
routes: []
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect("parse yaml");
        let err = config.build().expect_err("invalid endpoint ip");
        assert!(err.to_string().contains("Invalid endpoint IP"));
    }
}
