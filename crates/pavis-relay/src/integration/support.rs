use crate::runtime::{RelayOptions, RelayRuntimeState};
use crate::storage::validated_path::ValidatedStorageRoot;
use axum::body::Bytes;
use pavis_core::{AccessLogPolicy, ListenerName, Metrics, ServiceName, Telemetry, WorkerCount};
use std::net::SocketAddr;
use std::path::PathBuf;

pub(crate) fn temp_storage_root(label: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.join(format!("relay_integration_{label}_{pid}_{nanos}"))
}

pub(crate) fn state_with_storage(label: &str, version: u64, bytes: Bytes) -> RelayRuntimeState {
    let storage_root =
        ValidatedStorageRoot::new(temp_storage_root(label)).expect("validated storage root");
    let options = RelayOptions {
        storage_root,
        ..Default::default()
    };
    RelayRuntimeState::new_with_options(version, bytes, options).expect("state")
}

pub(crate) fn valid_pvs_bytes(label: &str) -> Bytes {
    let listener = pavis_core::ListenerBuilder::new()
        .name(ListenerName("default".to_string()))
        .address("127.0.0.1:8080".parse::<SocketAddr>().unwrap())
        .workers(WorkerCount::Auto)
        .tls(pavis_core::TlsConfig::Disabled)
        .build()
        .expect("listener");

    let config = pavis_core::RuntimeConfigBuilder::new()
        .telemetry(Telemetry {
            level: pavis_core::LogLevel::Info,
            pingora: pavis_core::LogLevel::Info,
            service_name: ServiceName(label.to_string()),
            metrics: Metrics::Disabled,
            access_log: AccessLogPolicy::Disabled,
            tracing: pavis_core::TracingPolicy::Disabled,
        })
        .add_listener(listener)
        .build()
        .expect("config");

    let validated = pavis_core::validate_runtime(config).expect("validate");
    Bytes::from(pavis_pvs::encode(validated.as_ref()).expect("encode"))
}
