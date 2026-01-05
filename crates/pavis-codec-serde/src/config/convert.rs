mod routes;
mod server;
mod telemetry;
mod upstreams;

use anyhow::Result;

use super::types::{SerdeConfig, StructurallyConfig};

pub fn structural_complete(src: SerdeConfig) -> StructurallyConfig {
    StructurallyConfig {
        listeners: src.listeners.unwrap_or_default(),
        telemetry: src.telemetry.unwrap_or_default(),
        upstreams: src.upstreams.unwrap_or_default(),
        routes: src.routes.unwrap_or_default(),
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

        Ok(pavis_core::RuntimeConfig {
            listeners,
            telemetry,
            upstreams,
            routes,
        })
    }
}

impl From<pavis_core::RuntimeConfig> for SerdeConfig {
    fn from(binary: pavis_core::RuntimeConfig) -> Self {
        let listeners = binary
            .listeners
            .into_iter()
            .map(server::from_runtime)
            .collect();
        SerdeConfig {
            listeners: Some(listeners),
            telemetry: Some(telemetry::from_runtime(binary.telemetry)),
            upstreams: Some(upstreams::from_runtime(binary.upstreams)),
            routes: Some(routes::from_runtime(binary.routes)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::SerdeFormat;
    use crate::config::types::SerdeConfig;
    use pavis_core::{
        AccessLogPolicy, ConnectTimeout, ConnectionLimit, Destination, Duration, Endpoint,
        EndpointAddr, Host, HttpVersion, IdleTimeout, Listener, ListenerName, LoadBalancer,
        LogLevel, Metrics, Path, PathMatch, Pool, Port, RETRY_CONNECT_FAILURE, RETRY_FIVE_XX,
        RetryFlags, RetryPolicy, Rewrite, RewriteHost, RewritePath, RuntimeConfig, ServiceName,
        Telemetry, Timeout, TlsConfig, TlsPolicy, TlsVerify, TracingPolicy, TracingProvider,
        TryTimeout, Upstream, UpstreamId, UpstreamName, VirtualHost, Weight, WorkerCount,
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
    endpoints:
      - address: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - matcher: !prefix
          path: "/"
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
        }
        match upstream.pool.connect {
            ConnectTimeout::Enabled(d) => assert_eq!(d.0.get(), 5_000),
            ConnectTimeout::Disabled => panic!("connect timeout not populated"),
        }
        match upstream.tls {
            TlsPolicy::Enabled { verify_mode, .. } => {
                assert_eq!(verify_mode, TlsVerify::CertAndHost);
            }
            TlsPolicy::Disabled => panic!("tls not enabled"),
        }

        let route = &runtime.routes[0].paths[0];
        match route.timeout {
            Timeout::Enabled(d) => assert_eq!(d.0.get(), 1000),
            Timeout::Disabled => panic!("route timeout not populated"),
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
        }
        match &route.request_headers {
            pavis_core::HeadersPolicy::Enabled { rules } => {
                assert_eq!(rules.set_headers.len(), 1);
                assert_eq!(rules.set_headers[0].0.0, "x-added");
                assert_eq!(rules.set_headers[0].1.0, "1");
            }
            _ => panic!("request headers not enabled"),
        }
        match &route.response_headers {
            pavis_core::HeadersPolicy::Enabled { rules } => {
                assert_eq!(rules.remove_headers.len(), 1);
                assert_eq!(rules.remove_headers[0].0, "x-remove");
            }
            _ => panic!("response headers not enabled"),
        }
    }

    #[test]
    fn runtime_to_yaml_preserves_values() {
        let runtime = RuntimeConfig {
            listeners: vec![Listener {
                name: ListenerName("default".to_string()),
                address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                workers: WorkerCount::Count(NonZeroU16::new(2).unwrap()),
                tls: TlsConfig::Disabled,
            }],
            telemetry: Telemetry {
                level: LogLevel::Info,
                pingora: LogLevel::Warn,
                service_name: ServiceName("svc".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Enabled {
                    provider: TracingProvider::Otlp,
                    sampling: pavis_core::SampleRate(10),
                },
            },
            upstreams: vec![Upstream {
                id: UpstreamId(NonZeroU16::new(7).unwrap()),
                name: UpstreamName("backend".to_string()),
                discovery: pavis_core::Discovery::Static,
                balancer: LoadBalancer::RoundRobin,
                protocol: HttpVersion::H2,
                pool: Pool {
                    idle: IdleTimeout::Enabled(Duration(NonZeroU32::new(10_000).unwrap())),
                    connect: ConnectTimeout::Enabled(Duration(NonZeroU32::new(2_000).unwrap())),
                    max: ConnectionLimit::Limited(NonZeroU32::new(10).unwrap()),
                },
                tls: TlsPolicy::Enabled {
                    verify_mode: TlsVerify::Cert,
                    sni: pavis_core::SniName::Value(pavis_core::Hostname(
                        "backend.local".to_string(),
                    )),
                },
                endpoints: vec![Endpoint {
                    address: EndpointAddr::Ip {
                        address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                        port: Port(NonZeroU16::new(8081).unwrap()),
                    },
                    weight: Weight(NonZeroU16::new(3).unwrap()),
                }],
            }],
            routes: vec![VirtualHost {
                host: Host("example.com".to_string()),
                paths: vec![pavis_core::Route {
                    matcher: PathMatch::Exact {
                        path: Path("/".to_string()),
                    },
                    timeout: Timeout::Enabled(Duration(NonZeroU32::new(1500).unwrap())),
                    retry: RetryPolicy::Enabled {
                        attempts: NonZeroU16::new(3).unwrap(),
                        per_try: TryTimeout::Enabled(Duration(NonZeroU32::new(500).unwrap())),
                        on: RetryFlags(RETRY_FIVE_XX),
                    },
                    request_headers: pavis_core::HeadersPolicy::Disabled,
                    response_headers: pavis_core::HeadersPolicy::Disabled,
                    rewrite: Rewrite {
                        path: RewritePath::Disabled,
                        host: RewriteHost::Disabled,
                    },
                    destinations: vec![Destination {
                        upstream: UpstreamName("backend".to_string()),
                        weight: Weight(NonZeroU16::new(2).unwrap()),
                    }],
                }],
            }],
        };

        let config: SerdeConfig = runtime.into();
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
