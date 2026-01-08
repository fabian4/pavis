use anyhow::Result;
use std::net::SocketAddr;

use pavis_core::{
    AccessLogPolicy, LogLevel, Metrics, SampleRate, ServiceName, Telemetry as RuntimeTelemetry,
    TracingPolicy, TracingProvider,
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
        access_log: telemetry.access_log.unwrap_or(AccessLogPolicy::Stdout),
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
            #[allow(unreachable_patterns)]
            _ => None,
        },
        access_log: Some(telemetry.access_log),
        tracing: match telemetry.tracing {
            TracingPolicy::Disabled => None,
            TracingPolicy::Enabled { provider, sampling } => {
                Some(crate::config::types::TracingConfig {
                    provider: Some(match provider {
                        TracingProvider::Otlp => "otlp".to_string(),
                        TracingProvider::Jaeger => "jaeger".to_string(),
                        TracingProvider::Zipkin => "zipkin".to_string(),
                        #[allow(unreachable_patterns)]
                        _ => "otlp".to_string(),
                    }),
                    sampling: Some(sampling.0),
                })
            }
            #[allow(unreachable_patterns)]
            _ => None,
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
        #[allow(unreachable_patterns)]
        _ => "info".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::{TelemetryConfig, TracingConfig};
    use pavis_core::{
        AccessLogPolicy, LogLevel, Metrics, SampleRate, ServiceName, Telemetry, TracingPolicy,
        TracingProvider,
    };
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn parse_log_level_handles_variants() {
        assert_eq!(
            parse_log_level(Some("debug".to_string())),
            Some(LogLevel::Debug)
        );
        assert_eq!(
            parse_log_level(Some("DEBUG".to_string())),
            Some(LogLevel::Debug)
        );
        assert_eq!(parse_log_level(Some("unknown".to_string())), None);
        assert_eq!(parse_log_level(None), None);
    }

    #[test]
    fn to_runtime_defaults() {
        let config = TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            metrics: None,
            access_log: None,
            tracing: None,
        };
        let runtime = to_runtime(config).unwrap();
        assert_eq!(runtime.level, LogLevel::Info);
        assert_eq!(runtime.service_name.0, "pavis");
    }

    #[test]
    fn to_runtime_validates_metrics_addr() {
        let config = TelemetryConfig {
            level: None,
            pingora: None,
            service_name: None,
            metrics: Some("invalid".to_string()),
            access_log: None,
            tracing: None,
        };
        let err = to_runtime(config).unwrap_err();
        assert!(err.to_string().contains("metrics must be a socket address"));
    }

    #[test]
    fn to_runtime_handles_tracing_providers() {
        let providers = vec![
            ("otlp", TracingProvider::Otlp),
            ("jaeger", TracingProvider::Jaeger),
            ("zipkin", TracingProvider::Zipkin),
            ("unknown", TracingProvider::Otlp), // Default
        ];

        for (input, expected) in providers {
            let config = TelemetryConfig {
                level: None,
                pingora: None,
                service_name: None,
                metrics: None,
                access_log: None,
                tracing: Some(TracingConfig {
                    provider: Some(input.to_string()),
                    sampling: Some(100),
                }),
            };
            let runtime = to_runtime(config).unwrap();
            match runtime.tracing {
                TracingPolicy::Enabled { provider, sampling } => {
                    let provider_matches = match expected {
                        TracingProvider::Otlp => matches!(provider, TracingProvider::Otlp),
                        TracingProvider::Jaeger => matches!(provider, TracingProvider::Jaeger),
                        TracingProvider::Zipkin => matches!(provider, TracingProvider::Zipkin),
                        _ => false,
                    };
                    assert!(provider_matches);
                    assert_eq!(sampling.0, 100);
                }
                _ => panic!("expected enabled tracing"),
            }
        }
    }

    #[test]
    fn from_runtime_round_trips_full_config() {
        let runtime = Telemetry {
            level: LogLevel::Debug,
            pingora: LogLevel::Warn,
            service_name: ServiceName("test-service".to_string()),
            metrics: Metrics::Enabled {
                addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9090),
            },
            access_log: AccessLogPolicy::Stdout,
            tracing: TracingPolicy::Enabled {
                provider: TracingProvider::Zipkin,
                sampling: SampleRate(50),
            },
        };

        let serde = from_runtime(runtime);
        assert_eq!(serde.level.as_deref(), Some("debug"));
        assert_eq!(serde.pingora.as_deref(), Some("warn"));
        assert_eq!(serde.service_name.as_deref(), Some("test-service"));
        assert_eq!(serde.metrics.as_deref(), Some("127.0.0.1:9090"));
        let tracing = serde.tracing.unwrap();
        assert_eq!(tracing.provider.as_deref(), Some("zipkin"));
        assert_eq!(tracing.sampling, Some(50));
    }
}
