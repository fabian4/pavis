use crate::header::format_address;
use crate::runtime::{
    AccessLogConfig, ConnectionPoolConfig, Endpoint, HttpVersion, LoadBalancer, MatchType, Route,
    RuntimeConfig, ServerConfig, TelemetryConfig, Upstream, VirtualHost, WeightedDestination,
};
use rkyv::check_archived_root;
use rkyv::ser::{Serializer, serializers::AllocSerializer};

fn create_valid_config() -> RuntimeConfig {
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
            access_log: AccessLogConfig::False,
            tracing: None,
        },
        upstreams: vec![Upstream {
            name: "test".to_string(),
            load_balancer: LoadBalancer::RoundRobin,
            http_version: HttpVersion::H1,
            connection_pool: ConnectionPoolConfig {
                idle_timeout_secs: 60,
                connection_timeout_secs: 5,
            },
            tls: None,
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
                request_headers: None,
                response_headers: None,
                destinations: vec![WeightedDestination {
                    upstream: "test".to_string(),
                    weight: 1,
                }],
                timeout_ms: None,
                retry_policy: None,
                compiled_regex: None,
            }],
        }],
    }
}

#[test]
fn test_validation_valid_data() {
    let config = create_valid_config();
    let mut serializer = AllocSerializer::<1024>::default();
    serializer.serialize_value(&config).unwrap();
    let bytes = serializer.into_serializer().into_inner();

    let result = check_archived_root::<RuntimeConfig>(&bytes);
    assert!(
        result.is_ok(),
        "Validation failed for valid data: {:?}",
        result.err()
    );
}

#[test]
fn test_validation_corrupted_data() {
    let config = create_valid_config();
    let mut serializer = AllocSerializer::<1024>::default();
    serializer.serialize_value(&config).unwrap();
    let mut bytes = serializer.into_serializer().into_inner().to_vec();

    if bytes.len() > 20 {
        for i in 10..20 {
            bytes[i] = 0xFF;
        }
    }

    let result = check_archived_root::<RuntimeConfig>(&bytes);
    assert!(
        result.is_err(),
        "Validation should have failed for corrupted data"
    );
}

#[test]
fn test_validation_truncated_data() {
    let config = create_valid_config();
    let mut serializer = AllocSerializer::<1024>::default();
    serializer.serialize_value(&config).unwrap();
    let bytes = serializer.into_serializer().into_inner();

    let truncated_bytes = &bytes[..bytes.len() / 2];

    let result = check_archived_root::<RuntimeConfig>(truncated_bytes);
    assert!(
        result.is_err(),
        "Validation should have failed for truncated data"
    );
}

#[test]
fn test_format_address() {
    assert_eq!(format_address("127.0.0.1", 8080), "127.0.0.1:8080");
    assert_eq!(format_address("::1", 80), "[::1]:80");
    assert_eq!(format_address("2001:db8::1", 443), "[2001:db8::1]:443");
    assert_eq!(format_address("[::1]", 80), "[::1]:80");
    assert_eq!(format_address("example.com", 80), "example.com:80");
    assert_eq!(format_address("localhost", 3000), "localhost:3000");
}
