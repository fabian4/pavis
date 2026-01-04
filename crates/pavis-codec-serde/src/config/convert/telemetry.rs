use anyhow::Result;
use std::net::SocketAddr;

use pavis_core::{
    LogLevel, Metrics, SampleRate, ServiceName, Telemetry as RuntimeTelemetry, TracingPolicy,
    TracingProvider,
};

use crate::config::types::TelemetryConfig;

pub(super) fn to_runtime(telemetry: TelemetryConfig) -> Result<RuntimeTelemetry> {
    let level = parse_log_level(telemetry.level).unwrap_or(LogLevel::Info);
    let pingora = parse_log_level(telemetry.pingora).unwrap_or(LogLevel::Info);
    let service_name = ServiceName(
        telemetry
            .service_name
            .unwrap_or_else(|| "pavis".to_string()),
    );

    let metrics = match telemetry.metrics {
        None => Metrics::Disabled,
        Some(addr) => Metrics::Enabled {
            addr: addr.parse::<SocketAddr>().map_err(|e| {
                anyhow::anyhow!("metrics must be a socket address (host:port): {}", e)
            })?,
        },
    };

    let tracing = match telemetry.tracing {
        None => TracingPolicy::Disabled,
        Some(tracing) => {
            let provider = tracing
                .provider
                .unwrap_or_else(|| "otlp".to_string())
                .to_lowercase();
            let provider = match provider.as_str() {
                "otlp" => TracingProvider::Otlp,
                "jaeger" => TracingProvider::Jaeger,
                "zipkin" => TracingProvider::Zipkin,
                _ => TracingProvider::Otlp,
            };
            let sampling = SampleRate(tracing.sampling.unwrap_or(0));
            TracingPolicy::Enabled { provider, sampling }
        }
    };

    Ok(RuntimeTelemetry {
        level,
        pingora,
        service_name,
        metrics,
        access_log: telemetry.access_log,
        tracing,
    })
}

pub(super) fn from_runtime(telemetry: RuntimeTelemetry) -> TelemetryConfig {
    TelemetryConfig {
        level: log_level_to_string(telemetry.level),
        pingora: log_level_to_string(telemetry.pingora),
        service_name: Some(telemetry.service_name.0),
        metrics: match telemetry.metrics {
            Metrics::Disabled => None,
            Metrics::Enabled { addr } => Some(addr.to_string()),
        },
        access_log: telemetry.access_log,
        tracing: match telemetry.tracing {
            TracingPolicy::Disabled => None,
            TracingPolicy::Enabled { provider, sampling } => {
                Some(crate::config::types::TracingConfig {
                    provider: Some(match provider {
                        TracingProvider::Otlp => "otlp".to_string(),
                        TracingProvider::Jaeger => "jaeger".to_string(),
                        TracingProvider::Zipkin => "zipkin".to_string(),
                    }),
                    sampling: Some(sampling.0),
                })
            }
        },
    }
}

fn parse_log_level(level: Option<String>) -> Option<LogLevel> {
    level.and_then(|l| match l.to_lowercase().as_str() {
        "error" => Some(LogLevel::Error),
        "warn" => Some(LogLevel::Warn),
        "info" => Some(LogLevel::Info),
        "debug" => Some(LogLevel::Debug),
        "trace" => Some(LogLevel::Trace),
        _ => None,
    })
}

fn log_level_to_string(level: LogLevel) -> Option<String> {
    Some(match level {
        LogLevel::Error => "error".to_string(),
        LogLevel::Warn => "warn".to_string(),
        LogLevel::Info => "info".to_string(),
        LogLevel::Debug => "debug".to_string(),
        LogLevel::Trace => "trace".to_string(),
    })
}
