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

#[cfg(test)]
mod tests {
    use super::{from_runtime, log_level_to_string, parse_log_level, to_runtime};
    use crate::config::types::{TelemetryConfig, TracingConfig};
    use pavis_core::{AccessLogConfig, LogLevel, TelemetryConfig as RuntimeTelemetryConfig};

    #[test]
    fn parse_log_level_handles_known_and_unknown_values() {
        assert_eq!(
            parse_log_level(Some("INFO".to_string())),
            Some(LogLevel::Info)
        );
        assert_eq!(parse_log_level(Some("unknown".to_string())), None);
        assert_eq!(parse_log_level(None), None);
    }

    #[test]
    fn log_level_to_string_maps_variants() {
        assert_eq!(
            log_level_to_string(Some(LogLevel::Error)),
            Some("error".to_string())
        );
        assert_eq!(
            log_level_to_string(Some(LogLevel::Warn)),
            Some("warn".to_string())
        );
    }

    #[test]
    fn telemetry_round_trips_tracing() {
        let telemetry = TelemetryConfig {
            level: Some("debug".to_string()),
            pingora: Some("trace".to_string()),
            service_name: Some("svc".to_string()),
            prometheus_addr: Some("0.0.0.0:9000".to_string()),
            access_log: AccessLogConfig::Stdout,
            tracing: Some(TracingConfig {
                enabled: true,
                provider: "otlp".to_string(),
                sampling_rate: 0.5,
            }),
        };
        let runtime = to_runtime(telemetry);
        let serde = from_runtime(RuntimeTelemetryConfig {
            level: runtime.level,
            pingora: runtime.pingora,
            service_name: runtime.service_name.clone(),
            prometheus_addr: runtime.prometheus_addr.clone(),
            access_log: runtime.access_log,
            tracing: runtime.tracing,
        });
        assert_eq!(serde.service_name.as_deref(), Some("svc"));
        assert_eq!(serde.tracing.as_ref().unwrap().provider, "otlp");
    }
}
