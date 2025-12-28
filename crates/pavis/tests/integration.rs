use pavis::router::Router;
use pavis::upstream::Manager;
use pavis_core::{
    AccessLogConfig, ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType, Route,
    RuntimeConfig as Config, ServerConfig, TelemetryConfig, Upstream, UpstreamTlsConfig,
    VirtualHost, WeightedDestination,
};

fn base_config() -> Config {
    Config {
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
            access_log: AccessLogConfig::False,
            tracing: None,
        },
        upstreams: vec![],
        routes: vec![],
    }
}

#[test]
fn test_configuration_driven_routing() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "backend-a".to_string(),
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![Endpoint {
            ip: "127.0.0.1".to_string(),
            port: 8081,
            weight: 1,
        }],
    });
    config.routes.push(VirtualHost {
        host: "*".to_string(),
        paths: vec![Route {
            match_type: MatchType::Prefix,
            path: "/api".to_string(),
            timeout_ms: None,
            retry_policy: None,
            request_headers: None,
            response_headers: None,
            destinations: vec![WeightedDestination {
                upstream: "backend-a".to_string(),
                weight: 1,
            }],
            compiled_regex: None,
        }],
    });

    let router = Router::new(config.routes).expect("Failed to create router");

    // Match /api
    let (_vhost, route) = router
        .match_request(None, "/api/users")
        .expect("Should match");
    assert_eq!(route.destinations[0].upstream, "backend-a");

    // No match
    assert!(router.match_request(None, "/other").is_none());
}

#[test]
fn test_load_balancer_state_correctness() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "backend-rr".to_string(),
        load_balancer: LoadBalancer::RoundRobin,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![
            Endpoint {
                ip: "10.0.0.1".to_string(),
                port: 80,
                weight: 1,
            },
            Endpoint {
                ip: "10.0.0.2".to_string(),
                port: 80,
                weight: 1,
            },
        ],
    });

    let manager = Manager::new(&config.upstreams);
    let cluster = manager.get("backend-rr").expect("Cluster not found");

    // Round robin should alternate
    let ep1 = cluster.select_endpoint().unwrap();
    let ep2 = cluster.select_endpoint().unwrap();
    let ep3 = cluster.select_endpoint().unwrap();

    assert_eq!(ep1.ip, "10.0.0.1");
    assert_eq!(ep2.ip, "10.0.0.2");
    assert_eq!(ep3.ip, "10.0.0.1");
}

#[test]
fn test_upstream_tls_config_parsing() {
    let mut config = base_config();
    config.upstreams.push(Upstream {
        name: "backend-secure".to_string(),
        load_balancer: LoadBalancer::Random,
        http_version: HttpVersion::H1,
        connection_pool: ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: Some(UpstreamTlsConfig {
            enabled: true,
            verify_hostname: false,
            verify_cert: false,
            sni: Some("secure.internal".to_string()),
        }),
        endpoints: vec![Endpoint {
            ip: "10.0.0.1".to_string(),
            port: 443,
            weight: 1,
        }],
    });

    let upstream = &config.upstreams[0];

    assert!(upstream.tls.is_some());
    let tls = upstream.tls.as_ref().unwrap();
    assert!(tls.enabled);
    assert_eq!(tls.verify_hostname, false);
    assert_eq!(tls.verify_cert, false);
    assert_eq!(tls.sni, Some("secure.internal".to_string()));
}
