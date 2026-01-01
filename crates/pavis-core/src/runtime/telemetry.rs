use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct TelemetryConfig {
    pub level: Option<LogLevel>,
    pub pingora: Option<LogLevel>,
    pub service_name: Option<String>,
    pub prometheus_addr: Option<String>,
    pub access_log: AccessLogConfig,
    pub tracing: Option<TracingConfig>,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[archive(check_bytes)]
#[repr(u8)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, PartialEq, Eq, Default)]
#[archive(check_bytes)]
pub enum AccessLogConfig {
    Disabled,
    #[default]
    Stdout,
    File(String),
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct TracingConfig {
    pub enabled: bool,
    pub provider: String,
    pub sampling_rate: f64,
}
