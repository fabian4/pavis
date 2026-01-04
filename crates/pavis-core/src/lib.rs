pub mod runtime;
pub mod validate;

pub use runtime::*;
pub use validate::{CoreValidationError, CoreValidationResult, validate_runtime};

#[cfg(feature = "serde")]
mod serde_impl;

#[cfg(test)]
mod tests {
    use super::{
        AccessLogPolicy, Listener, ListenerName, Metrics, RuntimeConfig, ServiceName, Telemetry,
        TracingPolicy, WorkerCount,
    };

    #[test]
    fn reexports_are_accessible() {
        let _cfg = RuntimeConfig {
            listeners: vec![Listener {
                name: ListenerName("default".to_string()),
                address: "127.0.0.1:8080".parse().expect("socket addr"),
                workers: WorkerCount::Auto,
                tls: super::TlsConfig::Disabled,
            }],
            telemetry: Telemetry {
                level: super::LogLevel::Info,
                pingora: super::LogLevel::Info,
                service_name: ServiceName("svc".to_string()),
                metrics: Metrics::Disabled,
                access_log: AccessLogPolicy::Disabled,
                tracing: TracingPolicy::Disabled,
            },
            upstreams: Vec::new(),
            routes: Vec::new(),
        };
    }
}
