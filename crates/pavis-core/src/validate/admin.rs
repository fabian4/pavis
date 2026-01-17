//! Validation for shutdown and admin configuration.

use crate::runtime::{AdminConfig, ShutdownPolicy};
use crate::validate::CoreValidationError;

/// Validate shutdown policy configuration.
///
/// Note: Warnings for excessively long drain_timeout (>5 minutes) should be
/// logged at the runtime/integration layer, not here.
pub(super) fn validate_shutdown(policy: &ShutdownPolicy) -> Result<(), CoreValidationError> {
    match policy {
        ShutdownPolicy::Disabled => Ok(()),
        ShutdownPolicy::Enabled { drain_timeout: _ } => {
            // All values are valid; warnings are handled by runtime layer
            Ok(())
        }
    }
}

/// Validate admin API configuration.
///
/// Note: Warnings for non-loopback bind addresses should be logged at the
/// runtime/integration layer, not here.
pub(super) fn validate_admin(config: &AdminConfig) -> Result<(), CoreValidationError> {
    match config {
        AdminConfig::Disabled => Ok(()),
        AdminConfig::Enabled { addr: _ } => {
            // All values are valid; warnings are handled by runtime layer
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Duration;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::NonZeroU32;

    fn duration_ms(ms: u32) -> Duration {
        Duration(NonZeroU32::new(ms).unwrap())
    }

    #[test]
    fn shutdown_disabled_validates() {
        let policy = ShutdownPolicy::Disabled;
        assert!(validate_shutdown(&policy).is_ok());
    }

    #[test]
    fn shutdown_enabled_validates() {
        let policy = ShutdownPolicy::Enabled {
            drain_timeout: duration_ms(30_000),
        };
        assert!(validate_shutdown(&policy).is_ok());
    }

    #[test]
    fn shutdown_long_timeout_validates() {
        // Long timeouts are valid (warnings logged at runtime layer)
        let policy = ShutdownPolicy::Enabled {
            drain_timeout: duration_ms(600_000), // 10 minutes
        };
        assert!(validate_shutdown(&policy).is_ok());
    }

    #[test]
    fn admin_disabled_validates() {
        let config = AdminConfig::Disabled;
        assert!(validate_admin(&config).is_ok());
    }

    #[test]
    fn admin_enabled_loopback_validates() {
        let config = AdminConfig::Enabled {
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9901),
        };
        assert!(validate_admin(&config).is_ok());
    }

    #[test]
    fn admin_enabled_non_loopback_validates() {
        // Non-loopback addresses are valid (warnings logged at runtime layer)
        let config = AdminConfig::Enabled {
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9901),
        };
        assert!(validate_admin(&config).is_ok());
    }
}
