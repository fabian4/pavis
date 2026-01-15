use pavis_core::{
    AccessLogPolicy, ListenerBuilder, ListenerName, Metrics, RuntimeConfig as Config,
    RuntimeConfigBuilder, ServiceName, Telemetry, TracingPolicy, WorkerCount,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub fn base_config() -> Config {
    let listener = ListenerBuilder::new()
        .name(ListenerName("default".to_string()))
        .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080))
        .workers(WorkerCount::Auto)
        .tls(pavis_core::TlsConfig::Disabled)
        .build()
        .expect("listener");

    RuntimeConfigBuilder::new()
        .telemetry(Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName("pavis".to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .build()
        .expect("config")
}
