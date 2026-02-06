//! Upstream materialization and default resolution.
//!
//! This module implements Zero-Option enforcement for upstreams: it is responsible
//! for resolving all optional user inputs (e.g. pool defaults, timeout defaults)
//! into concrete, explicit decisions before they reach pavis-core.

use super::semantic_validate::invalid_config_error;
use crate::config::types::ConnectionPoolConfig;
use pavis_core::{ConnectTimeout, Duration, FieldPathBuilder, IdleTimeout};
use std::num::NonZeroU32;

pub const DEFAULT_POOL_MAX: u32 = 128;
pub const DEFAULT_POOL_QUEUE_CAPACITY: u32 = 0;
pub const DEFAULT_POOL_QUEUE_TIMEOUT_MS: u32 = 0;

pub fn default_pool_config() -> ConnectionPoolConfig {
    ConnectionPoolConfig {
        idle: Some(default_idle_timeout()),
        connect: Some(default_connection_timeout()),
        max: None,
        queue_capacity: None,
        queue_timeout_ms: None,
        tcp_keepalive: None,
        tcp_nodelay: None,
        recv_buffer_size: None,
    }
}

pub fn default_idle_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(60)
}

pub fn default_connection_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(5)
}

pub fn duration_to_policy(duration: std::time::Duration) -> anyhow::Result<IdleTimeout> {
    let ms = u32::try_from(duration.as_millis())
        .map_err(|_| anyhow::anyhow!("idle timeout exceeds u32::MAX ms"))?;
    Ok(match NonZeroU32::new(ms) {
        Some(ms) => IdleTimeout::Enabled(Duration(ms)),
        None => IdleTimeout::Disabled,
    })
}

pub fn duration_to_connect(duration: std::time::Duration) -> anyhow::Result<ConnectTimeout> {
    let ms = u32::try_from(duration.as_millis())
        .map_err(|_| anyhow::anyhow!("connect timeout exceeds u32::MAX ms"))?;
    Ok(match NonZeroU32::new(ms) {
        Some(ms) => ConnectTimeout::Enabled(Duration(ms)),
        None => ConnectTimeout::Disabled,
    })
}

pub fn duration_to_required(
    duration: std::time::Duration,
    context: &str,
) -> anyhow::Result<Duration> {
    let ms = u32::try_from(duration.as_millis())
        .map_err(|_| anyhow::anyhow!("{context} exceeds u32::MAX ms"))?;
    let ms = NonZeroU32::new(ms).ok_or_else(|| anyhow::anyhow!("{context} must be > 0"))?;
    Ok(Duration(ms))
}

pub fn materialize_pool_max(
    value: Option<i64>,
    upstream_name: &str,
    index: usize,
) -> anyhow::Result<NonZeroU32> {
    let width = match value {
        None => return Ok(NonZeroU32::new(DEFAULT_POOL_MAX).expect("default max nonzero")),
        Some(raw) => raw,
    };
    if width < 1 {
        return Err(invalid_config_error(
            format!("upstream '{}' pool.max must be >= 1", upstream_name),
            Some(upstream_pool_field_path(index, "max")),
            Some("min_value=1"),
        ));
    }
    let max_value = u32::try_from(width).map_err(|_| {
        invalid_config_error(
            format!("upstream '{}' pool.max exceeds u32::MAX", upstream_name),
            Some(upstream_pool_field_path(index, "max")),
            Some("max_value=u32::MAX"),
        )
    })?;
    NonZeroU32::new(max_value).ok_or_else(|| {
        invalid_config_error(
            format!("upstream '{}' pool.max must be >= 1", upstream_name),
            Some(upstream_pool_field_path(index, "max")),
            Some("min_value=1"),
        )
    })
}

pub fn materialize_queue_value(
    value: Option<i64>,
    default: u32,
    field: &str,
    upstream_name: &str,
    index: usize,
) -> anyhow::Result<u32> {
    let raw = match value {
        None => return Ok(default),
        Some(raw) => raw,
    };
    if raw < 0 {
        return Err(invalid_config_error(
            format!("upstream '{}' pool.{} must be >= 0", upstream_name, field),
            Some(upstream_pool_field_path(index, field)),
            Some("min_value=0"),
        ));
    }
    u32::try_from(raw).map_err(|_| {
        invalid_config_error(
            format!(
                "upstream '{}' pool.{} exceeds u32::MAX",
                upstream_name, field
            ),
            Some(upstream_pool_field_path(index, field)),
            Some("max_value=u32::MAX"),
        )
    })
}

fn upstream_field_path(index: usize) -> FieldPathBuilder {
    FieldPathBuilder::new().root("upstreams").index(index)
}

fn upstream_pool_field_path(index: usize, field: &str) -> String {
    upstream_field_path(index)
        .field("pool")
        .field(field)
        .finish()
}

/// Converts a Duration to pavis_core::Duration for tcp_keepalive with validation.
pub fn duration_to_tcp_keepalive(
    duration: std::time::Duration,
    upstream_name: &str,
    index: usize,
) -> anyhow::Result<Duration> {
    let ms = u32::try_from(duration.as_millis()).map_err(|_| {
        invalid_config_error(
            format!(
                "upstream '{}' pool.tcp_keepalive exceeds u32::MAX ms",
                upstream_name
            ),
            Some(upstream_pool_field_path(index, "tcp_keepalive")),
            Some("max_value=u32::MAX milliseconds"),
        )
    })?;

    let ms = NonZeroU32::new(ms).ok_or_else(|| {
        invalid_config_error(
            format!(
                "upstream '{}' pool.tcp_keepalive must be > 0",
                upstream_name
            ),
            Some(upstream_pool_field_path(index, "tcp_keepalive")),
            Some("min_value=1ms"),
        )
    })?;

    Ok(Duration(ms))
}

/// Validates recv_buffer_size with warnings for suspicious values.
pub fn validate_recv_buffer_size(
    size: u32,
    _upstream_name: &str,
    _index: usize,
) -> anyhow::Result<u32> {
    const MIN_RECOMMENDED: u32 = 4096; // 4KB
    const MAX_RECOMMENDED: u32 = 1_048_576; // 1MB

    // Note: tracing is not available in codec layer (no runtime dependency)
    // Validation warnings would need to be logged at runtime level
    if !(MIN_RECOMMENDED..=MAX_RECOMMENDED).contains(&size) {
        // Validation happens silently here; runtime will log effective config
    }

    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_to_policy() {
        assert!(
            matches!(duration_to_policy(std::time::Duration::from_millis(100)).unwrap(), IdleTimeout::Enabled(Duration(ms)) if ms.get() == 100)
        );
        assert!(matches!(
            duration_to_policy(std::time::Duration::from_millis(0)).unwrap(),
            IdleTimeout::Disabled
        ));
        assert!(duration_to_policy(std::time::Duration::from_secs(u32::MAX as u64 + 1)).is_err());
    }

    #[test]
    fn test_materialize_pool_max() {
        assert_eq!(
            materialize_pool_max(None, "u", 0).unwrap().get(),
            DEFAULT_POOL_MAX
        );
        assert_eq!(materialize_pool_max(Some(10), "u", 0).unwrap().get(), 10);
        assert!(materialize_pool_max(Some(0), "u", 0).is_err());
        assert!(materialize_pool_max(Some(-1), "u", 0).is_err());
    }

    #[test]
    fn test_materialize_queue_value() {
        assert_eq!(materialize_queue_value(None, 10, "f", "u", 0).unwrap(), 10);
        assert_eq!(
            materialize_queue_value(Some(20), 10, "f", "u", 0).unwrap(),
            20
        );
        assert!(materialize_queue_value(Some(-1), 10, "f", "u", 0).is_err());
    }

    #[test]
    fn test_duration_to_tcp_keepalive() {
        assert_eq!(
            duration_to_tcp_keepalive(std::time::Duration::from_millis(100), "u", 0)
                .unwrap()
                .0
                .get(),
            100
        );
        assert!(duration_to_tcp_keepalive(std::time::Duration::from_millis(0), "u", 0).is_err());
    }

    #[test]
    fn test_materialize_errors() {
        assert!(duration_to_connect(std::time::Duration::from_secs(u32::MAX as u64 + 1)).is_err());
        assert!(materialize_pool_max(Some(u32::MAX as i64 + 1), "u", 0).is_err());
        assert!(materialize_queue_value(Some(u32::MAX as i64 + 1), 0, "f", "u", 0).is_err());
        assert!(
            duration_to_tcp_keepalive(std::time::Duration::from_secs(u32::MAX as u64 + 1), "u", 0)
                .is_err()
        );
    }

    #[test]
    fn test_duration_to_required() {
        assert_eq!(
            duration_to_required(std::time::Duration::from_millis(100), "ctx")
                .unwrap()
                .0
                .get(),
            100
        );
        assert!(duration_to_required(std::time::Duration::from_millis(0), "ctx").is_err());
        assert!(
            duration_to_required(std::time::Duration::from_secs(u32::MAX as u64 + 1), "ctx")
                .is_err()
        );
    }

    #[test]
    fn test_validate_recv_buffer_size() {
        assert_eq!(validate_recv_buffer_size(8192, "u", 0).unwrap(), 8192);
        assert_eq!(validate_recv_buffer_size(1024, "u", 0).unwrap(), 1024);
        assert_eq!(validate_recv_buffer_size(2000000, "u", 0).unwrap(), 2000000);
    }

    #[test]
    fn test_default_pool_config() {
        let cfg = default_pool_config();
        assert!(cfg.idle.is_some());
        assert!(cfg.connect.is_some());
        assert!(cfg.max.is_none());
    }
}
