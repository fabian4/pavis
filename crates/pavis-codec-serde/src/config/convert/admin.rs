use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::num::NonZeroU32;

use pavis_core::{AdminConfig, Duration, ShutdownPolicy};

use crate::config::types;

const DEFAULT_SHUTDOWN_ENABLED: bool = true;
const DEFAULT_SHUTDOWN_DRAIN_TIMEOUT_MS: u32 = 30_000;
const DEFAULT_ADMIN_ENABLED: bool = false;
const DEFAULT_ADMIN_ADDRESS: &str = "127.0.0.1:9901";

/// Convert shutdown DTO to runtime ShutdownPolicy.
///
/// Applies defaults:
/// - enabled: true (graceful shutdown on by default)
/// - drain_timeout_ms: 30000 (30 seconds)
pub fn shutdown_to_runtime(dto: types::ShutdownConfig) -> Result<ShutdownPolicy> {
    let enabled = dto.enabled.unwrap_or(DEFAULT_SHUTDOWN_ENABLED);

    if !enabled {
        return Ok(ShutdownPolicy::Disabled);
    }

    let drain_timeout_ms = dto
        .drain_timeout_ms
        .unwrap_or(DEFAULT_SHUTDOWN_DRAIN_TIMEOUT_MS);
    let drain_timeout = Duration(
        NonZeroU32::new(drain_timeout_ms)
            .context("drain_timeout_ms must be non-zero when shutdown is enabled")?,
    );

    Ok(ShutdownPolicy::Enabled { drain_timeout })
}

/// Convert admin API DTO to runtime AdminConfig.
///
/// Applies defaults:
/// - enabled: false (admin API off by default)
/// - address: "127.0.0.1:9901"
pub fn admin_to_runtime(dto: types::AdminConfig) -> Result<AdminConfig> {
    let enabled = dto.enabled.unwrap_or(DEFAULT_ADMIN_ENABLED);

    if !enabled {
        return Ok(AdminConfig::Disabled);
    }

    let addr_str = dto.address.as_deref().unwrap_or(DEFAULT_ADMIN_ADDRESS);
    let addr: SocketAddr = addr_str
        .parse()
        .with_context(|| format!("Invalid admin address: {}", addr_str))?;

    Ok(AdminConfig::Enabled { addr })
}

/// Convert runtime ShutdownPolicy to shutdown DTO.
pub fn shutdown_from_runtime(policy: ShutdownPolicy) -> types::ShutdownConfig {
    match policy {
        ShutdownPolicy::Disabled => types::ShutdownConfig {
            enabled: Some(false),
            drain_timeout_ms: None,
        },
        ShutdownPolicy::Enabled { drain_timeout } => types::ShutdownConfig {
            enabled: Some(true),
            drain_timeout_ms: Some(drain_timeout.0.get()),
        },
        #[allow(unreachable_patterns)]
        _ => types::ShutdownConfig {
            enabled: Some(false),
            drain_timeout_ms: None,
        },
    }
}

/// Convert runtime AdminConfig to admin API DTO.
pub fn admin_from_runtime(config: AdminConfig) -> types::AdminConfig {
    match config {
        AdminConfig::Disabled => types::AdminConfig {
            enabled: Some(false),
            address: None,
        },
        AdminConfig::Enabled { addr } => types::AdminConfig {
            enabled: Some(true),
            address: Some(addr.to_string()),
        },
        #[allow(unreachable_patterns)]
        _ => types::AdminConfig {
            enabled: Some(false),
            address: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_default_enabled() {
        let dto = types::ShutdownConfig::default();
        let policy = shutdown_to_runtime(dto).expect("default shutdown");
        match policy {
            ShutdownPolicy::Enabled { drain_timeout } => {
                assert_eq!(drain_timeout.0.get(), DEFAULT_SHUTDOWN_DRAIN_TIMEOUT_MS);
            }
            ShutdownPolicy::Disabled => panic!("expected enabled by default"),
            #[allow(unreachable_patterns)]
            _ => panic!("unexpected shutdown policy"),
        }
    }

    #[test]
    fn shutdown_default_timeout_30s() {
        let dto = types::ShutdownConfig {
            enabled: Some(true),
            drain_timeout_ms: None,
        };
        let policy = shutdown_to_runtime(dto).expect("shutdown with default timeout");
        match policy {
            ShutdownPolicy::Enabled { drain_timeout } => {
                assert_eq!(drain_timeout.0.get(), 30_000);
            }
            ShutdownPolicy::Disabled => panic!("expected enabled"),
            #[allow(unreachable_patterns)]
            _ => panic!("unexpected shutdown policy"),
        }
    }

    #[test]
    fn shutdown_explicit_disabled() {
        let dto = types::ShutdownConfig {
            enabled: Some(false),
            drain_timeout_ms: None,
        };
        let policy = shutdown_to_runtime(dto).expect("disabled shutdown");
        assert!(matches!(policy, ShutdownPolicy::Disabled));
    }

    #[test]
    fn shutdown_custom_timeout() {
        let dto = types::ShutdownConfig {
            enabled: Some(true),
            drain_timeout_ms: Some(60_000),
        };
        let policy = shutdown_to_runtime(dto).expect("custom timeout");
        match policy {
            ShutdownPolicy::Enabled { drain_timeout } => {
                assert_eq!(drain_timeout.0.get(), 60_000);
            }
            ShutdownPolicy::Disabled => panic!("expected enabled"),
            #[allow(unreachable_patterns)]
            _ => panic!("unexpected shutdown policy"),
        }
    }

    #[test]
    fn shutdown_zero_timeout_fails() {
        let dto = types::ShutdownConfig {
            enabled: Some(true),
            drain_timeout_ms: Some(0),
        };
        let err = shutdown_to_runtime(dto).expect_err("zero timeout should fail");
        assert!(err.to_string().contains("non-zero"));
    }

    #[test]
    fn admin_default_disabled() {
        let dto = types::AdminConfig::default();
        let config = admin_to_runtime(dto).expect("default admin");
        assert!(matches!(config, AdminConfig::Disabled));
    }

    #[test]
    fn admin_explicit_enabled() {
        let dto = types::AdminConfig {
            enabled: Some(true),
            address: None,
        };
        let config = admin_to_runtime(dto).expect("enabled admin");
        match config {
            AdminConfig::Enabled { addr } => {
                assert_eq!(addr.to_string(), DEFAULT_ADMIN_ADDRESS);
            }
            AdminConfig::Disabled => panic!("expected enabled"),
            #[allow(unreachable_patterns)]
            _ => panic!("unexpected admin config"),
        }
    }

    #[test]
    fn admin_custom_address() {
        let dto = types::AdminConfig {
            enabled: Some(true),
            address: Some("0.0.0.0:9999".to_string()),
        };
        let config = admin_to_runtime(dto).expect("custom address");
        match config {
            AdminConfig::Enabled { addr } => {
                assert_eq!(addr.to_string(), "0.0.0.0:9999");
            }
            AdminConfig::Disabled => panic!("expected enabled"),
            #[allow(unreachable_patterns)]
            _ => panic!("unexpected admin config"),
        }
    }

    #[test]
    fn admin_invalid_address_fails() {
        let dto = types::AdminConfig {
            enabled: Some(true),
            address: Some("invalid".to_string()),
        };
        let err = admin_to_runtime(dto).expect_err("invalid address should fail");
        assert!(err.to_string().contains("Invalid admin address"));
    }

    #[test]
    fn shutdown_round_trip() {
        let policy = ShutdownPolicy::Enabled {
            drain_timeout: Duration(NonZeroU32::new(45_000).unwrap()),
        };
        let dto = shutdown_from_runtime(policy);
        let converted = shutdown_to_runtime(dto).expect("round trip");
        assert!(
            matches!((policy, converted), (ShutdownPolicy::Enabled { drain_timeout: d1 }, ShutdownPolicy::Enabled { drain_timeout: d2 }) if d1.0.get() == d2.0.get())
        );
    }

    #[test]
    fn admin_round_trip() {
        let config = AdminConfig::Enabled {
            addr: "192.168.1.1:8080".parse().unwrap(),
        };
        let dto = admin_from_runtime(config);
        let converted = admin_to_runtime(dto).expect("round trip");
        match (config, converted) {
            (AdminConfig::Enabled { addr: a1 }, AdminConfig::Enabled { addr: a2 }) => {
                assert_eq!(a1, a2);
            }
            #[allow(unreachable_patterns)]
            _ => panic!("round trip failed"),
        }
    }
}
