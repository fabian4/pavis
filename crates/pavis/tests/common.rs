use pavis_core::{AccessLogConfig, RuntimeConfig as Config, ServerConfig, TelemetryConfig};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub fn base_config() -> Config {
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
            access_log: AccessLogConfig::Disabled,
            tracing: None,
        },
        upstreams: vec![],
        routes: vec![],
    }
}
