use anyhow::Result;
use pavis_core::{
    AccessLogPolicy, Listener, ListenerName, LogLevel, Metrics, RuntimeConfig, ServiceName,
    Telemetry, TlsConfig, TracingPolicy, WorkerCount,
};
use pavis_e2e::support::PavisScenario;
use pavis_e2e::support::relay::RelayOptions;
use std::time::Duration;
use tokio::time::sleep;

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
            service_name: ServiceName("long-poll-test".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: TracingPolicy::Disabled,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    }
}

#[tokio::test]
async fn r3_long_poll_semantics() -> Result<()> {
    let scenario = PavisScenario::new(RelayOptions::default(), false, false).await?;
    let client = scenario.relay.client();

    let status = client.status().await?;
    let current_version = status.version;

    // Start long poll in background
    let client_clone = client.clone();
    let poll_handle =
        tokio::spawn(async move { client_clone.long_poll(current_version, 2000).await });

    // Wait a bit, then publish
    sleep(Duration::from_millis(500)).await;

    let config = simple_config();
    client.publish(&config).await?;

    // Long poll should return immediately after publish
    let result = poll_handle.await.unwrap()?;
    assert!(result.is_some());
    assert_eq!(result.unwrap().0, current_version + 1);

    Ok(())
}
