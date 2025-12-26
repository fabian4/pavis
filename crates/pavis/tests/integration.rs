use pavis::config::Config;
use pavis::router::Router;
use pavis::upstream::Manager;

#[test]
fn test_configuration_driven_routing() {
    let yaml = include_str!("fixtures/routing.yaml");
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
    let yaml = include_str!("fixtures/load_balancing.yaml");
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

#[test]
fn test_upstream_tls_config_parsing() {
    let yaml = include_str!("fixtures/upstream_tls.yaml");
    let config: Config = serde_yaml::from_str(yaml).expect("Failed to parse config");
    let upstream = &config.upstreams[0];

    assert!(upstream.tls.is_some());
    let tls = upstream.tls.as_ref().unwrap();
    assert!(tls.enabled);
    assert_eq!(tls.verify_hostname, Some(false));
    assert_eq!(tls.verify_cert, Some(false));
    assert_eq!(tls.sni, Some("secure.internal".to_string()));
}
