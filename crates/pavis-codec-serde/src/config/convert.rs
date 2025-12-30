mod routes;
mod server;
mod telemetry;
mod upstreams;

use anyhow::Result;

use pavis_core::validate_runtime;

use super::types::SerdeConfig;

impl TryFrom<SerdeConfig> for pavis_core::RuntimeConfig {
    type Error = anyhow::Error;

    fn try_from(src: SerdeConfig) -> Result<Self, Self::Error> {
        let server = server::to_runtime(src.server)?;
        let telemetry = telemetry::to_runtime(src.telemetry);
        let upstreams = upstreams::to_runtime(src.upstreams)?;
        let routes = routes::to_runtime(src.routes)?;

        let runtime = pavis_core::RuntimeConfig {
            server,
            telemetry,
            upstreams,
            routes,
        };

        validate_runtime(runtime.clone()).map_err(anyhow::Error::from)?;
        Ok(runtime)
    }
}

impl From<pavis_core::RuntimeConfig> for SerdeConfig {
    fn from(binary: pavis_core::RuntimeConfig) -> Self {
        SerdeConfig {
            server: server::from_runtime(binary.server),
            telemetry: telemetry::from_runtime(binary.telemetry),
            upstreams: upstreams::from_runtime(binary.upstreams),
            routes: routes::from_runtime(binary.routes),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::SerdeFormat;
    use crate::config::types::SerdeConfig;
    use pavis_core::{
        AccessLogConfig, ConnectionPoolConfig, Endpoint, HeaderOperations, HttpVersion,
        LoadBalancer, LogLevel, MatchType, RetryPolicy, Route, RuntimeConfig, ServerConfig,
        TelemetryConfig, Upstream, UpstreamTlsConfig, VirtualHost, WeightedDestination,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;

    #[test]
    fn yaml_to_runtime_converts_defaults_and_structures() {
        let yaml = r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    tls:
      enabled: true
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "example.com"
    paths:
      - path: "/"
        timeout: "1s"
        request_headers:
          add:
            x-added: "1"
        response_headers:
          remove: ["x-remove"]
        retry:
          attempts: 2
          per_try_timeout: "250ms"
          retry_on: ["5xx", "connect-failure"]
        destinations:
          - upstream: "backend"
            weight: 1
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect("parse yaml");
        let runtime: RuntimeConfig = config.try_into().expect("convert to runtime");

        let upstream = &runtime.upstreams[0];
        assert_eq!(upstream.endpoints[0].weight, 1);
        assert_eq!(upstream.connection_pool.idle_timeout_secs, 60);
        assert_eq!(upstream.connection_pool.connection_timeout_secs, 5);
        let tls = upstream.tls.as_ref().expect("tls config");
        assert!(tls.verify_hostname);
        assert!(tls.verify_cert);

        let route = &runtime.routes[0].paths[0];
        assert_eq!(route.timeout_ms, Some(1000));
        let retry = route.retry_policy.as_ref().expect("retry policy");
        assert_eq!(retry.attempts, 2);
        assert_eq!(retry.per_try_timeout_ms, 250);
        assert_eq!(
            retry.retry_on,
            vec!["5xx".to_string(), "connect-failure".to_string()]
        );
        let request_headers = route.request_headers.as_ref().expect("request headers");
        assert_eq!(
            request_headers.add,
            vec![("x-added".to_string(), "1".to_string())]
        );
        assert!(request_headers.remove.is_empty());
        let response_headers = route.response_headers.as_ref().expect("response headers");
        assert!(response_headers.add.is_empty());
        assert_eq!(response_headers.remove, vec!["x-remove".to_string()]);
    }

    #[test]
    fn runtime_to_yaml_preserves_values() {
        let runtime = RuntimeConfig {
            server: ServerConfig {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                worker_threads: Some(2),
                tls: None,
            },
            telemetry: TelemetryConfig {
                level: Some(LogLevel::Info),
                pingora: None,
                service_name: None,
                prometheus_addr: None,
                access_log: AccessLogConfig::Disabled,
                tracing: None,
            },
            upstreams: vec![Upstream {
                name: "backend".to_string(),
                load_balancer: LoadBalancer::RoundRobin,
                http_version: HttpVersion::H2,
                connection_pool: ConnectionPoolConfig {
                    idle_timeout_secs: 10,
                    connection_timeout_secs: 2,
                },
                tls: Some(UpstreamTlsConfig {
                    enabled: false,
                    verify_hostname: false,
                    verify_cert: false,
                    sni: Some("backend.local".to_string()),
                }),
                endpoints: vec![Endpoint {
                    ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port: 8081,
                    weight: 3,
                }],
            }],
            routes: vec![VirtualHost {
                host: "example.com".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Exact,
                    path: "/".to_string(),
                    timeout_ms: Some(1500),
                    retry_policy: Some(RetryPolicy {
                        attempts: 3,
                        per_try_timeout_ms: 500,
                        retry_on: vec!["5xx".to_string()],
                    }),
                    request_headers: Some(HeaderOperations {
                        add: vec![("x-add".to_string(), "1".to_string())],
                        remove: vec!["x-remove".to_string()],
                    }),
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend".to_string(),
                        weight: 2,
                    }],
                }],
            }],
        };

        let config: SerdeConfig = runtime.into();
        assert_eq!(config.server.listen_addr, "127.0.0.1:8080");
        assert_eq!(config.server.worker_threads, Some(2));
        assert_eq!(config.telemetry.level, Some("info".to_string()));
        assert_eq!(config.telemetry.access_log, AccessLogConfig::Disabled);
        let upstream = &config.upstreams[0];
        assert_eq!(upstream.load_balancer, LoadBalancer::RoundRobin);
        assert_eq!(upstream.http_version, HttpVersion::H2);
        assert_eq!(
            upstream.connection_pool.idle_timeout,
            Duration::from_secs(10)
        );
        assert_eq!(
            upstream.connection_pool.connection_timeout,
            Duration::from_secs(2)
        );
        let tls = upstream.tls.as_ref().expect("tls config");
        assert_eq!(tls.enabled, false);
        assert_eq!(tls.verify_hostname, Some(false));
        assert_eq!(tls.verify_cert, Some(false));
        assert_eq!(tls.sni.as_deref(), Some("backend.local"));
        assert_eq!(upstream.endpoints[0].weight, Some(3));
        assert_eq!(upstream.endpoints[0].ip, "127.0.0.1");

        let route = &config.routes[0].paths[0];
        assert_eq!(route.match_type, MatchType::Exact);
        assert_eq!(route.timeout, Some(Duration::from_millis(1500)));
        let retry = route.retry.as_ref().expect("retry policy");
        assert_eq!(retry.attempts, 3);
        assert_eq!(retry.per_try_timeout, Duration::from_millis(500));
        assert_eq!(
            retry.retry_on,
            vec![serde_json::Value::String("5xx".to_string())]
        );
        let request_headers = route.request_headers.as_ref().expect("request headers");
        assert_eq!(
            request_headers.add.as_ref().expect("add headers")["x-add"],
            "1"
        );
        assert_eq!(
            request_headers.remove.as_ref().expect("remove headers"),
            &vec!["x-remove".to_string()]
        );
    }

    #[test]
    fn yaml_to_runtime_rejects_invalid_listen_addr() {
        let yaml = r#"
server:
  listen_addr: "invalid-addr"
telemetry: {}
upstreams: []
routes: []
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect("parse yaml");
        let err = pavis_core::RuntimeConfig::try_from(config).expect_err("invalid listen addr");
        assert!(err.to_string().contains("Invalid listen_addr"));
    }

    #[test]
    fn yaml_to_runtime_rejects_invalid_endpoint_ip() {
        let yaml = r#"
server:
  listen_addr: "127.0.0.1:8080"
telemetry: {}
upstreams:
  - name: "backend"
    endpoints:
      - ip: "not-an-ip"
        port: 8081
routes: []
"#;

        let config = SerdeConfig::parse_str(SerdeFormat::Yaml, yaml).expect("parse yaml");
        let err = pavis_core::RuntimeConfig::try_from(config).expect_err("invalid endpoint ip");
        assert!(err.to_string().contains("Invalid endpoint IP"));
    }
}
