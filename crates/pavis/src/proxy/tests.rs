use crate::proxy::header_ops::{apply_request_headers, apply_response_headers};
use crate::router::Router;
use crate::upstream::Cluster;
use pavis_core::{
    AccessLogConfig, HeaderOperations, LoadBalancer, MatchType, Route, RuntimeConfig as Config,
    ServerConfig, TelemetryConfig, VirtualHost, WeightedDestination,
};
use pingora::http::{RequestHeader, ResponseHeader};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

fn create_test_config() -> Config {
    Config {
        server: ServerConfig {
            listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080),
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
        routes: vec![
            VirtualHost {
                host: "example.com".to_string(),
                paths: vec![
                    Route {
                        match_type: MatchType::Exact,
                        path: "/exact".to_string(),
                        timeout_ms: None,
                        retry_policy: None,
                        request_headers: None,
                        response_headers: None,
                        destinations: vec![WeightedDestination {
                            upstream: "backend-1".to_string(),
                            weight: 1,
                        }],
                        compiled_regex: None,
                    },
                    Route {
                        match_type: MatchType::Prefix,
                        path: "/api".to_string(),
                        timeout_ms: None,
                        retry_policy: None,
                        request_headers: None,
                        response_headers: None,
                        destinations: vec![WeightedDestination {
                            upstream: "backend-1".to_string(),
                            weight: 1,
                        }],
                        compiled_regex: None,
                    },
                ],
            },
            VirtualHost {
                host: "*".to_string(),
                paths: vec![Route {
                    match_type: MatchType::Prefix,
                    path: "/public".to_string(),
                    timeout_ms: None,
                    retry_policy: None,
                    request_headers: None,
                    response_headers: None,
                    destinations: vec![WeightedDestination {
                        upstream: "backend-2".to_string(),
                        weight: 1,
                    }],
                    compiled_regex: None,
                }],
            },
        ],
    }
}

#[test]
fn test_find_route_exact_match() {
    let config = create_test_config();
    let router = Router::new(config.routes.clone()).unwrap();
    let (vhost, route) = router
        .match_request(Some("example.com"), "/exact")
        .expect("Should match");
    assert_eq!(vhost.host, "example.com");
    assert_eq!(route.path, "/exact");
}

#[test]
fn test_find_route_prefix_match() {
    let config = create_test_config();
    let router = Router::new(config.routes.clone()).unwrap();
    let (vhost, route) = router
        .match_request(Some("example.com"), "/api/v1/users")
        .expect("Should match");
    assert_eq!(vhost.host, "example.com");
    assert_eq!(route.path, "/api");
}

#[test]
fn test_find_route_wildcard_host() {
    let config = create_test_config();
    let router = Router::new(config.routes.clone()).unwrap();
    let (vhost, route) = router
        .match_request(Some("any.com"), "/public/stuff")
        .expect("Should match");
    assert_eq!(vhost.host, "*");
    assert_eq!(route.path, "/public");
}

#[test]
fn test_find_route_no_match() {
    let config = create_test_config();
    let router = Router::new(config.routes.clone()).unwrap();
    let result = router.match_request(Some("example.com"), "/notfound");
    assert!(result.is_none());
}

#[test]
fn test_find_route_wrong_host() {
    let config = create_test_config();
    let router = Router::new(config.routes.clone()).unwrap();
    let result = router.match_request(Some("other.com"), "/exact");
    assert!(result.is_none());
}

#[test]
fn test_find_route_regex_match() {
    let config = Config {
        server: ServerConfig {
            listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080),
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
        routes: vec![VirtualHost {
            host: "*".to_string(),
            paths: vec![Route {
                match_type: MatchType::Regex,
                path: r"^/api/v[0-9]+/users/\d+$".to_string(),
                timeout_ms: None,
                retry_policy: None,
                request_headers: None,
                response_headers: None,
                destinations: vec![WeightedDestination {
                    upstream: "backend".to_string(),
                    weight: 1,
                }],
                compiled_regex: None,
            }],
        }],
    };

    let router = Router::new(config.routes.clone()).unwrap();

    // Should match
    let result = router.match_request(None, "/api/v1/users/123");
    assert!(result.is_some());
    let (_, route) = result.unwrap();
    assert_eq!(route.match_type, MatchType::Regex);

    // Should match v2
    let result = router.match_request(None, "/api/v2/users/456");
    assert!(result.is_some());

    // Should NOT match (missing user id)
    let result = router.match_request(None, "/api/v1/users/");
    assert!(result.is_none());

    // Should NOT match (non-numeric user id)
    let result = router.match_request(None, "/api/v1/users/abc");
    assert!(result.is_none());
}

#[test]
fn test_weighted_round_robin_respects_weights() {
    let upstream = pavis_core::Upstream {
        name: "test".to_string(),
        load_balancer: LoadBalancer::RoundRobin,
        http_version: pavis_core::HttpVersion::H1,
        connection_pool: pavis_core::ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![
            pavis_core::Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: 8080,
                weight: 3,
            },
            pavis_core::Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                port: 8081,
                weight: 1,
            },
        ],
    };

    let cluster = Cluster::new(upstream);

    let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));

    assert_eq!(cluster.select_endpoint().unwrap().ip, ip1); // 0
    assert_eq!(cluster.select_endpoint().unwrap().ip, ip1); // 1
    assert_eq!(cluster.select_endpoint().unwrap().ip, ip1); // 2
    assert_eq!(cluster.select_endpoint().unwrap().ip, ip2); // 3
    assert_eq!(cluster.select_endpoint().unwrap().ip, ip1); // 4
}

#[test]
fn test_round_robin_cycles_endpoints_evenly() {
    let upstream = pavis_core::Upstream {
        name: "test-upstream".to_string(),
        load_balancer: LoadBalancer::RoundRobin,
        http_version: pavis_core::HttpVersion::H1,
        connection_pool: pavis_core::ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![
            pavis_core::Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: 8081,
                weight: 1,
            },
            pavis_core::Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: 8082,
                weight: 1,
            },
            pavis_core::Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: 8083,
                weight: 1,
            },
        ],
    };

    let cluster = Cluster::new(upstream);

    let e1 = cluster.select_endpoint().unwrap();
    assert_eq!(e1.port, 8081);

    let e2 = cluster.select_endpoint().unwrap();
    assert_eq!(e2.port, 8082);

    let e3 = cluster.select_endpoint().unwrap();
    assert_eq!(e3.port, 8083);

    let e4 = cluster.select_endpoint().unwrap();
    assert_eq!(e4.port, 8081);
}

#[test]
fn test_concurrent_round_robin() {
    let upstream = pavis_core::Upstream {
        name: "concurrent-upstream".to_string(),
        load_balancer: LoadBalancer::RoundRobin,
        http_version: pavis_core::HttpVersion::H1,
        connection_pool: pavis_core::ConnectionPoolConfig {
            idle_timeout_secs: 60,
            connection_timeout_secs: 5,
        },
        tls: None,
        endpoints: vec![
            pavis_core::Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                port: 80,
                weight: 1,
            },
            pavis_core::Endpoint {
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
                port: 80,
                weight: 1,
            },
        ],
    };

    let cluster = Arc::new(Cluster::new(upstream));

    let mut handles = vec![];
    for _ in 0..10 {
        let c = cluster.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                let _ = c.select_endpoint();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let count = cluster
        .rr_counter
        .0
        .load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(count, 1000);
}

#[test]
fn test_apply_headers() {
    let mut req = RequestHeader::build("GET", b"/", None).unwrap();
    req.insert_header("X-Remove", "old-value").unwrap();

    let mut add_list = Vec::new();
    add_list.push(("X-Add".to_string(), "new-value".to_string()));

    let remove_list = vec!["X-Remove".to_string()];

    let ops = HeaderOperations {
        add: add_list,
        remove: remove_list,
    };

    apply_request_headers(&mut req, Some(&ops)).unwrap();

    assert_eq!(
        req.headers.get("X-Proxy-By").unwrap().to_str().unwrap(),
        "Pavis"
    );
    assert_eq!(
        req.headers.get("X-Add").unwrap().to_str().unwrap(),
        "new-value"
    );
    assert!(req.headers.get("X-Remove").is_none());
}

#[test]
fn test_apply_response_headers() {
    let mut resp = ResponseHeader::build(200, None).unwrap();
    resp.insert_header("X-Remove-Resp", "bad-value").unwrap();

    let mut add_list = Vec::new();
    add_list.push(("X-Add-Resp".to_string(), "good-value".to_string()));

    let remove_list = vec!["X-Remove-Resp".to_string()];

    let ops = HeaderOperations {
        add: add_list,
        remove: remove_list,
    };

    apply_response_headers(&mut resp, Some(&ops)).unwrap();

    assert_eq!(
        resp.headers.get("X-Proxy-By").unwrap().to_str().unwrap(),
        "Pavis"
    );
    assert_eq!(
        resp.headers.get("X-Add-Resp").unwrap().to_str().unwrap(),
        "good-value"
    );
    assert!(resp.headers.get("X-Remove-Resp").is_none());
}
