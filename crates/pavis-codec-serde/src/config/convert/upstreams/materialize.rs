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
