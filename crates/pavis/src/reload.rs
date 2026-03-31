//! Hot-reload safety checks for config fields that still require boot-time wiring.

use anyhow::bail;
use pavis_core::ValidatedRuntimeConfig;

/// Fields outside `RuntimeState` require boot-time service wiring and are not safe
/// to change through the current hot-reload path.
pub(crate) fn ensure_reload_safe(
    current: &ValidatedRuntimeConfig,
    next: &ValidatedRuntimeConfig,
) -> anyhow::Result<()> {
    let mut changed = Vec::new();

    if listener_fingerprint(current) != listener_fingerprint(next) {
        changed.push("listeners");
    }
    if format!("{:?}", current.admin) != format!("{:?}", next.admin) {
        changed.push("admin");
    }
    if format!("{:?}", current.shutdown) != format!("{:?}", next.shutdown) {
        changed.push("shutdown");
    }
    if format!("{:?}", current.telemetry.metrics) != format!("{:?}", next.telemetry.metrics) {
        changed.push("telemetry.metrics");
    }
    if format!("{:?}", current.telemetry.access_log) != format!("{:?}", next.telemetry.access_log) {
        changed.push("telemetry.access_log");
    }

    if changed.is_empty() {
        return Ok(());
    }

    bail!(
        "reload rejected: boot-time config changed ({})",
        changed.join(", ")
    )
}

fn listener_fingerprint(config: &ValidatedRuntimeConfig) -> Vec<String> {
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
    use super::ensure_reload_safe;
    use pavis_core::{
        AccessLogPolicy, AdminConfig, ListenerBuilder, ListenerName, Metrics, RuntimeConfigBuilder,
        ServiceName, ShutdownPolicy, Telemetry, TracingPolicy, WorkerCount,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn base_config() -> pavis_core::ValidatedRuntimeConfig {
        let listener = ListenerBuilder::new()
            .name(ListenerName("default".to_string()))
            .address(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080))
            .workers(WorkerCount::Auto)
            .tls(pavis_core::TlsConfig::Disabled)
            .build()
            .expect("listener");

        pavis_core::validate_runtime(
            RuntimeConfigBuilder::new()
                .telemetry(Telemetry {
                    level: pavis_core::LogLevel::Info,
                    pingora: pavis_core::LogLevel::Info,
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
        let next = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(next) };
        assert!(ensure_reload_safe(&current, &next).is_ok());
    }

    #[test]
    fn rejects_listener_changes() {
        let current = base_config();
        let mut next = current.clone().into_inner();
        next.listeners[0].address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9090);
        let next = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(next) };
        let err = ensure_reload_safe(&current, &next).expect_err("listener change rejected");
        assert!(err.to_string().contains("listeners"));
    }

    #[test]
    fn rejects_access_log_changes() {
        let current = base_config();
        let mut next = current.clone().into_inner();
        next.telemetry.access_log = AccessLogPolicy::Stdout;
        let next = unsafe { pavis_core::ValidatedRuntimeConfig::from_trusted(next) };
        let err = ensure_reload_safe(&current, &next).expect_err("access log change rejected");
        assert!(err.to_string().contains("telemetry.access_log"));
    }
}
