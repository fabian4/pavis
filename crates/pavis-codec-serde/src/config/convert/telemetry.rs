use pavis_core::{LogLevel, TelemetryConfig as RuntimeTelemetryConfig, TracingConfig};

use crate::config::types::TelemetryConfig;

pub(super) fn to_runtime(telemetry: TelemetryConfig) -> RuntimeTelemetryConfig {
    RuntimeTelemetryConfig {
        level: parse_log_level(telemetry.level),
        pingora: parse_log_level(telemetry.pingora),
        service_name: telemetry.service_name,
        prometheus_addr: telemetry.prometheus_addr,
        access_log: telemetry.access_log,
        tracing: telemetry.tracing.map(|t| TracingConfig {
            enabled: t.enabled,
            provider: t.provider,
            sampling_rate: t.sampling_rate,
        }),
    }
}

pub(super) fn from_runtime(telemetry: RuntimeTelemetryConfig) -> TelemetryConfig {
    TelemetryConfig {
        level: log_level_to_string(telemetry.level),
        pingora: log_level_to_string(telemetry.pingora),
        service_name: telemetry.service_name,
        prometheus_addr: telemetry.prometheus_addr,
        access_log: telemetry.access_log,
        tracing: telemetry
            .tracing
            .map(|t| crate::config::types::TracingConfig {
                enabled: t.enabled,
                provider: t.provider,
                sampling_rate: t.sampling_rate,
            }),
    }
}

fn parse_log_level(level: Option<String>) -> Option<LogLevel> {
    level.and_then(|l| match l.to_lowercase().as_str() {
        "error" => Some(LogLevel::Error),
        "warn" => Some(LogLevel::Warn),
        "info" => Some(LogLevel::Info),
        "debug" => Some(LogLevel::Debug),
        "trace" => Some(LogLevel::Trace),
        _ => None, // Fallback to None (or could error, but Option is safe)
    })
}

fn log_level_to_string(level: Option<LogLevel>) -> Option<String> {
    level.map(|l| match l {
        LogLevel::Error => "error".to_string(),
        LogLevel::Warn => "warn".to_string(),
        LogLevel::Info => "info".to_string(),
        LogLevel::Debug => "debug".to_string(),
        LogLevel::Trace => "trace".to_string(),
    })
}
