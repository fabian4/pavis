use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
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
}

#[derive(Debug, Deserialize, Default)]
pub struct IdentityConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cluster: String,
    #[serde(default)]
    pub instance_id: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct HttpConfig {
    #[serde(default)]
    pub bind: String,
    #[serde(default)]
    pub admin_bind: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    #[serde(default)]
    pub storage_type: String,
    #[serde(default)]
    pub root_dir: String,
}

#[derive(Debug, Deserialize, Default)]
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

#[derive(Debug, Deserialize, Default)]
pub struct ArtifactLimits {
    #[serde(default)]
    pub max_pvs_bytes: u64,
    #[serde(default)]
    pub max_routes: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub ingest: IngestConfig,
    #[serde(default)]
    pub codec: CodecConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct IngestConfig {
    #[serde(default)]
    pub source: IngestSource,
    #[serde(default)]
    pub mode: IngestMode,
}

#[derive(Debug, Deserialize, Default)]
pub struct IngestSource {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub config: serde_yaml::Value,
}

#[derive(Debug, Deserialize, Default)]
pub struct IngestMode {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub config: serde_yaml::Value,
}

#[derive(Debug, Deserialize, Default)]
pub struct CodecConfig {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub options: CodecOptions,
}

#[derive(Debug, Deserialize, Default)]
pub struct CodecOptions {
    #[serde(default)]
    pub strict_unknown_fields: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct ExecutionConfig {
    #[serde(default)]
    pub versioning: VersioningConfig,
    #[serde(default)]
    pub publish: PublishConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct VersioningConfig {
    #[serde(default)]
    pub scheme: String,
    #[serde(default)]
    pub state_file: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct PublishConfig {
    #[serde(default)]
    pub atomic_write: bool,
    #[serde(default)]
    pub fsync: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct DistributionConfig {
    #[serde(default)]
    pub long_poll: LongPollConfig,
    #[serde(default)]
    pub direct_fetch: DirectFetchConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct LongPollConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub headers: LongPollHeaders,
    #[serde(default)]
    pub timeouts: LongPollTimeouts,
}

#[derive(Debug, Deserialize, Default)]
pub struct LongPollHeaders {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub checksum: String,
    #[serde(default)]
    pub algorithm: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct LongPollTimeouts {
    #[serde(default)]
    pub hold_seconds: u64,
    #[serde(default)]
    pub idle_seconds: u64,
}

#[derive(Debug, Deserialize, Default)]
pub struct DirectFetchConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub bearer: BearerConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct BearerConfig {
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct LoggingConfig {
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub access_log: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct MetricsConfig {
    #[serde(default)]
    pub prometheus_bind: String,
}
