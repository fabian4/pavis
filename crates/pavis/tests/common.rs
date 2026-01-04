use pavis_core::{AccessLogConfig, Listener, RuntimeConfig as Config, TelemetryConfig};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub fn base_config() -> Config {
    Config {
        listeners: vec![Listener {
            name: "default".to_string(),
            listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080),
            worker_threads: None,
            tls: None,
        }],
        telemetry: TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            prometheus_addr: None,
            access_log: AccessLogConfig::Disabled,
            tracing: None,
        },
        upstreams: vec![],
        routes: vec![],
    }
}
