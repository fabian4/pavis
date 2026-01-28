use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

use pavis::bootstrap::{BootstrapOptions, BootstrapPlan};
use pavis::load::{self, RuntimeLoadError};
use pavis::validate_env;
use pavis_core::LogLevel;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
    #[arg(long)]
    relay_url: Option<String>,
}

fn log_level_to_str(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
        #[allow(unreachable_patterns)]
        _ => "info", // Default to info for unknown log levels
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration (LKG)
    let config = load::load_file(&args.config).map_err(|e| match e {
        RuntimeLoadError::Pvs(pavis_pvs::PvsError::VersionMismatch { file, expected }) => {
            anyhow::anyhow!(
                "Version mismatch! File: {}, Runtime expects: {}. Recompile config.",
                file,
                expected
            )
        }
        other => anyhow::anyhow!(other),
    })?;

    validate_env::validate_runtime_env(&config, None)?;
    let config = Arc::new(config);

    // Setup logging (Subscriber + Reloadable OpenTelemetry Layer)
    let log_level = log_level_to_str(config.telemetry.level);
    let mut filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    let p_str = log_level_to_str(config.telemetry.pingora);
    filter = filter
        .add_directive(format!("pingora={}", p_str).parse()?)
        .add_directive(format!("pingora_core={}", p_str).parse()?);

    let fmt_layer = tracing_subscriber::fmt::layer();

    // Custom reloadable layer. transparent to downcasting.
    let reload_handle: pavis::telemetry::tracing::ReloadHandle =
        pavis::telemetry::tracing::ReloadableLayer::new();
    let otel_layer = reload_handle.clone();

    // Register layers. Order matters: otel_layer is typed for Registry,
    // so it must be applied directly to Registry.
    Registry::default()
        .with(otel_layer)
        .with(fmt_layer)
        .with(filter)
        .init();
    let bootstrap_plan = BootstrapPlan::build(
        config.clone(),
        reload_handle.clone(),
        BootstrapOptions {
            config_path: PathBuf::from(&args.config),
            relay_url: args.relay_url.clone(),
        },
    )?;
    bootstrap_plan.run()
}

#[cfg(test)]
mod tests {
    use super::log_level_to_str;
    use pavis_core::LogLevel;

    #[test]
    fn log_level_to_str_defaults_to_info() {
        assert_eq!(log_level_to_str(LogLevel::Info), "info");
    }

    #[test]
    fn test_log_level_to_str_all() {
        assert_eq!(log_level_to_str(LogLevel::Error), "error");
        assert_eq!(log_level_to_str(LogLevel::Warn), "warn");
        assert_eq!(log_level_to_str(LogLevel::Info), "info");
        assert_eq!(log_level_to_str(LogLevel::Debug), "debug");
        assert_eq!(log_level_to_str(LogLevel::Trace), "trace");
    }
}
