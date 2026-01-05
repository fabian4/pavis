use pavis_core::{AccessLogPolicy, ListenerName, Metrics, ServiceName, Telemetry, WorkerCount};
use pavis_e2e::support::{RelayInstance, RelayOptions};
use std::net::SocketAddr;

fn simple_config() -> pavis_core::RuntimeConfig {
    pavis_core::RuntimeConfig {
        listeners: vec![pavis_core::Listener {
            name: ListenerName("default".to_string()),
            address: "127.0.0.1:0".parse::<SocketAddr>().unwrap(),
            workers: WorkerCount::Auto,
            tls: pavis_core::TlsConfig::Disabled,
        }],
        telemetry: Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName("traceability-test".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        },
        upstreams: vec![],
        routes: vec![],
    }
}

#[tokio::test]
async fn verify_generated_at_header() {
    let instance = RelayInstance::new(RelayOptions::default())
        .await
        .expect("relay instance");
    let client = instance.client();
    let start_version = client.status().await.expect("status").version;

    // 1. Publish a config
    let config = simple_config();
    let publish_resp = client.publish(&config).await.expect("publish");
    assert_eq!(publish_resp.version, start_version + 1);

    // 2. Fetch config and check header (simulating a client poll)
    // RelayClient.long_poll abstracts away headers, so we use internal reqwest client to inspect headers
    let raw_client = reqwest::Client::new();
    let relay_url = instance.env.base_url();

    let resp = raw_client
        .get(format!("{}/v1/config", relay_url))
        .header("X-Pavis-Version", "0")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert!(headers.contains_key("x-pavis-generated-at"));
    let val = headers
        .get("x-pavis-generated-at")
        .unwrap()
        .to_str()
        .unwrap();
    chrono::DateTime::parse_from_rfc3339(val).expect("valid rfc3339 timestamp");

    // 3. Fetch specific artifact
    let resp = raw_client
        .get(format!(
            "{}/v1/artifacts/{}",
            relay_url, publish_resp.version
        ))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert!(headers.contains_key("x-pavis-generated-at"));
    let val = headers
        .get("x-pavis-generated-at")
        .unwrap()
        .to_str()
        .unwrap();
    chrono::DateTime::parse_from_rfc3339(val).expect("valid rfc3339 timestamp");
}
