use pavis_codec_serde::SerdeFormat;
use pavis_codec_serde::config::SerdeConfig;
use pavis_codec_serde::serde_helpers::emit_with_format;
use pavis_core::{
    AccessLogPolicy, Listener, ListenerName, Metrics, RuntimeConfig, ServiceName, Telemetry,
    TracingPolicy, WorkerCount,
};

#[allow(dead_code)]
pub fn to_yaml(config: &RuntimeConfig) -> String {
    let config: SerdeConfig = config.clone().into();
    let bytes = emit_with_format(SerdeFormat::Yaml, &config).expect("encode config to yaml");
    String::from_utf8(bytes).expect("utf8 config")
}

pub fn build_pvs_bytes(label: &str) -> Vec<u8> {
    let config = RuntimeConfig {
        listeners: vec![Listener {
            name: ListenerName("default".to_string()),
            address: "127.0.0.1:8080".parse().expect("addr"),
            workers: WorkerCount::Auto,
            tls: pavis_core::TlsConfig::Disabled,
        }],
        telemetry: Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName(label.to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Stdout,
            tracing: TracingPolicy::Disabled,
        },
        upstreams: Vec::new(),
        routes: Vec::new(),
    };

    let dir = std::env::temp_dir();
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();
    let path = dir.join(format!("pavis_pvs_helper_{label}_{pid}_{id}.pvs"));
    pavis_pvs::write(&path, &config).expect("write config");
    let bytes = std::fs::read(&path).expect("read config");
    let _ = std::fs::remove_file(&path);
    bytes
}
