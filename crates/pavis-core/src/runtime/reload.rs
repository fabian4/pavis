use super::{RuntimeConfig, ValidatedRuntimeConfig};
use thiserror::Error;

/// Canonical reload-boundary check for the current split boot/runtime architecture.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("reload rejected: boot-time config changed ({fields})", fields = .changed_fields.join(", "))]
pub struct ReloadSafetyError {
    changed_fields: Vec<&'static str>,
}

impl ReloadSafetyError {
    pub fn changed_fields(&self) -> &[&'static str] {
        &self.changed_fields
    }
}

pub fn ensure_runtime_reload_safe(
    current: &ValidatedRuntimeConfig,
    next: &ValidatedRuntimeConfig,
) -> Result<(), ReloadSafetyError> {
    let mut changed_fields = Vec::new();

    if listener_fingerprint(current.as_ref()) != listener_fingerprint(next.as_ref()) {
        changed_fields.push("listeners");
    }
    if format!("{:?}", current.admin) != format!("{:?}", next.admin) {
        changed_fields.push("admin");
    }
    if format!("{:?}", current.shutdown) != format!("{:?}", next.shutdown) {
        changed_fields.push("shutdown");
    }
    if format!("{:?}", current.telemetry.metrics) != format!("{:?}", next.telemetry.metrics) {
        changed_fields.push("telemetry.metrics");
    }
    if format!("{:?}", current.telemetry.access_log) != format!("{:?}", next.telemetry.access_log) {
        changed_fields.push("telemetry.access_log");
    }

    if changed_fields.is_empty() {
        return Ok(());
    }

    Err(ReloadSafetyError { changed_fields })
}

fn listener_fingerprint(config: &RuntimeConfig) -> Vec<String> {
    config
        .listeners
        .iter()
        .map(|listener| {
            format!(
                "{}|{}|{:?}|{:?}",
                listener.name.0, listener.address, listener.workers, listener.tls
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::ensure_runtime_reload_safe;
    use crate::{
        AccessLogPolicy, AdminConfig, ListenerBuilder, ListenerName, Metrics, RuntimeConfigBuilder,
        ServiceName, ShutdownPolicy, Telemetry, TracingPolicy, WorkerCount, validate_runtime,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn base_config() -> crate::ValidatedRuntimeConfig {
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
            .workers(WorkerCount::Auto)
            .tls(crate::TlsConfig::Disabled)
            .build()
            .expect("listener");

        validate_runtime(
            RuntimeConfigBuilder::new()
                .telemetry(Telemetry {
                    level: crate::LogLevel::Info,
                    pingora: crate::LogLevel::Info,
                    service_name: ServiceName("svc".to_string()),
                    metrics: Metrics::Disabled,
                    access_log: AccessLogPolicy::Disabled,
                    tracing: TracingPolicy::Disabled,
                })
                .shutdown(ShutdownPolicy::Disabled)
                .admin(AdminConfig::Disabled)
                .add_listener(listener)
                .build()
                .expect("config"),
        )
        .expect("validated")
    }

    #[test]
    fn accepts_runtime_only_changes() {
        let current = base_config();
        let mut next = current.clone().into_inner();
        next.telemetry.service_name.0 = "svc-next".to_string();
        let next = unsafe { crate::ValidatedRuntimeConfig::from_trusted(next) };
        assert!(ensure_runtime_reload_safe(&current, &next).is_ok());
    }

    #[test]
    fn rejects_listener_changes() {
        let current = base_config();
        let mut next = current.clone().into_inner();
        next.listeners[0].address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9090);
        let next = unsafe { crate::ValidatedRuntimeConfig::from_trusted(next) };
        let err =
            ensure_runtime_reload_safe(&current, &next).expect_err("listener change rejected");
        assert_eq!(err.changed_fields(), ["listeners"]);
    }

    #[test]
    fn rejects_multiple_boot_time_changes() {
        let current = base_config();
        let mut next = current.clone().into_inner();
        next.telemetry.access_log = AccessLogPolicy::Stdout;
        next.admin = AdminConfig::Enabled {
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9091),
        };
        let next = unsafe { crate::ValidatedRuntimeConfig::from_trusted(next) };
        let err =
            ensure_runtime_reload_safe(&current, &next).expect_err("boot-time change rejected");
        assert_eq!(err.changed_fields(), ["admin", "telemetry.access_log"]);
    }
}
