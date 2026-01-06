use anyhow::Result;
use pavis_core::{
    AccessLogPolicy, Listener, ListenerName, LogLevel, Metrics, RuntimeConfig, ServiceName,
    Telemetry, TlsConfig, TracingPolicy, WorkerCount,
};
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;

fn simple_config() -> RuntimeConfig {
    RuntimeConfig {
        listeners: vec![Listener {
            name: ListenerName("default".to_string()),
            address: "127.0.0.1:0".parse().unwrap(),
            workers: WorkerCount::Auto,
            tls: TlsConfig::Disabled,
        }],
        telemetry: Telemetry {
            level: LogLevel::Info,
            pingora: LogLevel::Info,
            service_name: ServiceName("traceability-test".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: TracingPolicy::Disabled,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    }
}

#[tokio::test]
async fn r16_traceability() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();
    let start_version = client.status().await?.version;

    // 1. Publish a config
    let config = simple_config();
    let publish_resp = client.publish(&config).await?;
    assert_eq!(publish_resp.version, start_version + 1);

    // 2. Fetch config and check header
    let raw_client = reqwest::Client::new();
    let relay_url = scenario.relay.env.base_url();

    let resp = raw_client
        .get(format!("{}/v1/config", relay_url))
        .header("X-Pavis-Version", "0")
        .send()
        .await?;
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert!(headers.contains_key("x-pavis-generated-at"));

    let val = headers.get("x-pavis-generated-at").unwrap().to_str()?;
    chrono::DateTime::parse_from_rfc3339(val).expect("valid rfc3339 timestamp");

    Ok(())
}
