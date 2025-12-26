use pavis::config::Config;
use pavis::router::Router;
use pavis::upstream::Manager;

#[test]
fn test_configuration_driven_routing() {
    let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: false
upstreams:
  - name: "backend-a"
    type: "static"
    endpoints:
      - ip: "127.0.0.1"
        port: 8081
routes:
  - host: "*"
    paths:
      - path: "/api"
        destinations:
          - upstream: "backend-a"
            weight: 1
"#;
    let config: Config = serde_yaml::from_str(yaml).expect("Failed to parse config");
    let router = Router::new(&config.routes).expect("Failed to create router");

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
    let yaml = r#"
server:
  listen_addr: "0.0.0.0:8080"
telemetry:
  access_log: false
upstreams:
  - name: "backend-rr"
    type: "static"
    load_balancer: "round-robin"
    endpoints:
      - ip: "10.0.0.1"
        port: 80
      - ip: "10.0.0.2"
        port: 80
routes: []
"#;
    let config: Config = serde_yaml::from_str(yaml).expect("Failed to parse config");
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
