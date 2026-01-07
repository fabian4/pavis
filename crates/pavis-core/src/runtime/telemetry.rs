use crate::runtime::types::{Path, SampleRate, ServiceName};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
pub struct Telemetry {
    pub level: LogLevel,
    pub pingora: LogLevel,
    pub service_name: ServiceName,
    pub metrics: Metrics,
    pub access_log: AccessLogPolicy,
    pub tracing: TracingPolicy,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[archive(check_bytes)]
#[repr(u8)]
#[non_exhaustive]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone, PartialEq, Eq, Default)]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum AccessLogPolicy {
    Disabled,
    #[default]
    Stdout,
    File(Path),
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum TracingPolicy {
    Disabled,
    Enabled {
        provider: TracingProvider,
        sampling: SampleRate,
    },
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum TracingProvider {
    Otlp,
    Jaeger,
    Zipkin,
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[archive(check_bytes)]
#[non_exhaustive]
pub enum Metrics {
    Disabled,
    Enabled { addr: SocketAddr },
}
