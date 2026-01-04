pub mod runtime;
pub mod validate;

pub use runtime::*;
pub use validate::{CoreValidationError, CoreValidationResult, validate_runtime};

#[cfg(feature = "serde")]
mod serde_impl;

#[cfg(test)]
mod tests {
    use super::{AccessLogConfig, Listener, RuntimeConfig, TelemetryConfig};

    #[test]
    fn reexports_are_accessible() {
        let _cfg = RuntimeConfig {
            listeners: vec![Listener {
                name: "default".to_string(),
                listen_addr: "127.0.0.1:8080".parse().expect("socket addr"),
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
            upstreams: Vec::new(),
            routes: Vec::new(),
        };
    }
}
