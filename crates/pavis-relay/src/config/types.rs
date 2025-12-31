use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct RelayConfig {
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub http: HttpConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub artifact: ArtifactConfig,
    #[serde(default)]
    pub pipeline: PipelineConfig,
    #[serde(default)]
    pub distribution: DistributionConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub persistence: PersistenceConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct IdentityConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cluster: String,
    #[serde(default)]
    pub instance_id: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct HttpConfig {
    #[serde(default)]
    pub bind: String,
    #[serde(default)]
    pub admin_bind: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    #[serde(default)]
    pub storage_type: String,
    #[serde(default)]
    pub root_dir: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ArtifactConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pvs_filename: String,
    #[serde(default)]
    pub lkg_path: String,
    #[serde(default)]
    pub artifacts_dir: String,
    #[serde(default)]
    pub limits: ArtifactLimits,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PersistenceConfig {
    #[serde(default = "PersistenceConfig::default_enabled")]
    pub enabled: bool,
    #[serde(default = "PersistenceConfig::default_flush_interval")]
    pub flush_interval: u64,
    #[serde(default)]
    pub retry: RetryConfig,
}

impl PersistenceConfig {
    pub fn default_enabled() -> bool {
        true
    }
    pub fn default_flush_interval() -> u64 {
        1_000
    }
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            flush_interval: Self::default_flush_interval(),
            retry: RetryConfig::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PersistenceOptions {
    pub enabled: bool,
    pub flush_interval: Duration,
    pub retry_max: u32,
    pub retry_backoff: Duration,
    pub retry_backoff_max: Duration,
}

impl Default for PersistenceOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            flush_interval: Duration::from_secs(1),
            retry_max: 5,
            retry_backoff: Duration::from_millis(250),
            retry_backoff_max: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct RetryConfig {
    #[serde(default = "RetryConfig::default_max")]
    pub max: u32,
    #[serde(default)]
    pub backoff: RetryBackoffConfig,
}

impl RetryConfig {
    pub fn default_max() -> u32 {
        5
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max: Self::default_max(),
            backoff: RetryBackoffConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ArtifactLimits {
    #[serde(default)]
    pub max_pvs_bytes: u64,
    #[serde(default)]
    pub max_routes: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub ingest: IngestConfig,
    #[serde(default)]
    pub codec: CodecConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub runtime: PipelineRuntimeConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct IngestConfig {
    #[serde(default)]
    pub source: IngestSource,
    #[serde(default)]
    pub mode: IngestMode,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
#[serde(rename_all = "lowercase")]
pub enum IngestSource {
    File(FileSourceConfig),
}

impl Default for IngestSource {
    fn default() -> Self {
        IngestSource::File(FileSourceConfig::default())
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct FileSourceConfig {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct IngestMode {
    #[serde(default)]
    pub kind: String,
    #[serde(default = "IngestMode::default_debounce")]
    pub debounce: u64,
}

impl IngestMode {
    pub fn default_debounce() -> u64 {
        100
    }
}

impl Default for IngestMode {
    fn default() -> Self {
        Self {
            kind: String::new(),
            debounce: Self::default_debounce(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CodecConfig {
    #[serde(default)]
    pub kind: CodecKind,
    #[serde(default)]
    pub mode: CodecModeConfig,
}

impl Default for CodecConfig {
    fn default() -> Self {
        Self {
            kind: CodecKind::Serde,
            mode: CodecModeConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CodecKind {
    #[default]
    Serde,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct CodecModeConfig {
    #[serde(default)]
    pub compaction: PipelineCompaction,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ExecutionConfig {
    #[serde(default)]
    pub versioning: VersioningConfig,
    #[serde(default)]
    pub publish: PublishConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PipelineRuntimeConfig {
    #[serde(default = "PipelineRuntimeConfig::default_max_in_flight")]
    pub max_in_flight: usize,
    #[serde(default)]
    pub restart_backoff: RestartBackoffConfig,
    #[serde(default)]
    pub publish_retry: PublishRetryConfig,
}

impl PipelineRuntimeConfig {
    pub fn default_max_in_flight() -> usize {
        8
    }
}

impl Default for PipelineRuntimeConfig {
    fn default() -> Self {
        Self {
            max_in_flight: Self::default_max_in_flight(),
            restart_backoff: RestartBackoffConfig::default(),
            publish_retry: PublishRetryConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum PipelineCompaction {
    #[default]
    Off,
    Trim,
    Prune,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct RestartBackoffConfig {
    #[serde(default = "RestartBackoffConfig::default_min")]
    pub min: u64,
    #[serde(default = "RestartBackoffConfig::default_max")]
    pub max: u64,
}

impl RestartBackoffConfig {
    pub fn default_min() -> u64 {
        500
    }
    pub fn default_max() -> u64 {
        30_000
    }
}

impl Default for RestartBackoffConfig {
    fn default() -> Self {
        Self {
            min: Self::default_min(),
            max: Self::default_max(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct RetryBackoffConfig {
    #[serde(default = "RetryBackoffConfig::default_min")]
    pub min: u64,
    #[serde(default = "RetryBackoffConfig::default_max")]
    pub max: u64,
}

impl RetryBackoffConfig {
    pub fn default_min() -> u64 {
        250
    }
    pub fn default_max() -> u64 {
        5_000
    }
}

impl Default for RetryBackoffConfig {
    fn default() -> Self {
        Self {
            min: Self::default_min(),
            max: Self::default_max(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PublishRetryConfig {
    #[serde(default = "PublishRetryConfig::default_max")]
    pub max: u32,
    #[serde(default)]
    pub backoff: RetryBackoffConfig,
}

impl PublishRetryConfig {
    pub fn default_max() -> u32 {
        5
    }
}

impl Default for PublishRetryConfig {
    fn default() -> Self {
        Self {
            max: Self::default_max(),
            backoff: RetryBackoffConfig::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PipelineOptions {
    pub max_in_flight: usize,
    pub compaction: PipelineCompaction,
    pub restart_backoff: BackoffConfig,
    pub publish_retry: RetryPolicy,
}

impl PipelineOptions {
    pub fn from_config(config: &PipelineConfig) -> Self {
        let runtime = &config.runtime;
        Self {
            max_in_flight: runtime.max_in_flight,
            compaction: config.codec.mode.compaction,
            restart_backoff: BackoffConfig {
                base_delay: Duration::from_millis(runtime.restart_backoff.min),
                max_delay: Duration::from_millis(runtime.restart_backoff.max),
            },
            publish_retry: RetryPolicy {
                max_attempts: runtime.publish_retry.max,
                base_delay: Duration::from_millis(runtime.publish_retry.backoff.min),
                max_delay: Duration::from_millis(runtime.publish_retry.backoff.max),
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BackoffConfig {
    pub base_delay: Duration,
    pub max_delay: Duration,
}

#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct VersioningConfig {
    #[serde(default)]
    pub scheme: String,
    #[serde(default)]
    pub state_file: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct PublishConfig {
    #[serde(default)]
    pub atomic_write: bool,
    #[serde(default)]
    pub fsync: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct DistributionConfig {
    #[serde(default)]
    pub long_poll: LongPollConfig,
    #[serde(default)]
    pub direct_fetch: DirectFetchConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct LongPollConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub timeouts: LongPollTimeouts,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct LongPollTimeouts {
    #[serde(default)]
    pub hold_seconds: u64,
    #[serde(default)]
    pub idle_seconds: u64,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct DirectFetchConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub bearer: BearerConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct BearerConfig {
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct LoggingConfig {
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub access_log: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct MetricsConfig {
    #[serde(default)]
    pub prometheus_bind: String,
}
