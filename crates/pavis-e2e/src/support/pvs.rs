use pavis_codec_api::Codec;
use pavis_codec_serde::{SerdeCodec, SerdeFormat};
use pavis_core::{RuntimeConfig, ServerConfig, TelemetryConfig};

pub fn to_yaml(config: &RuntimeConfig) -> String {
    let codec = SerdeCodec {
        format: SerdeFormat::Yaml,
    };
    let artifact = codec.pack(config).expect("pack config to yaml");
    String::from_utf8(artifact.bytes.to_vec()).expect("utf8 config")
}

pub fn build_pvs_bytes(label: &str) -> Vec<u8> {
    let config = RuntimeConfig {
        server: ServerConfig {
            listen_addr: "127.0.0.1:8080".parse().expect("addr"),
            worker_threads: None,
            tls: None,
        },
        telemetry: TelemetryConfig {
            level: None,
            pingora: None,
            service_name: Some(label.to_string()),
            prometheus_addr: None,
            access_log: Default::default(),
            tracing: None,
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
