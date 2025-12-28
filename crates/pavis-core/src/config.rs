use crate::RuntimeConfig;
use std::path::Path;

/// Represents the source from which a configuration can be loaded.
#[derive(Debug, Clone)]
pub enum ConfigSource<'a> {
    /// A file path (e.g., for YAML or PVS files).
    File(&'a Path),
    /// A raw string content (e.g., YAML/JSON string).
    String(&'a str),
    /// A binary buffer (e.g., raw bytes).
    Bytes(&'a [u8]),
    // Future variants:
    // XdsStream(Url),
    // Database(ConnectionInfo),
}

/// A common interface for configuration input types (adapters).
///
/// This trait standardizes the lifecycle of an input configuration (like `YamlConfig`
/// or `XdsConfig`) as it moves through the pipeline:
/// 1. `load`: Ingest raw data from a source into the input struct.
/// 2. `validate`: Perform source-specific validation (schema, types).
/// 3. `build`: Convert the validated input into the canonical `RuntimeConfig`.
pub trait Config {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Loads the configuration from a specific source.
    fn load(source: ConfigSource) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Validates the configuration state.
    ///
    /// This step should check for input-specific constraints (e.g., valid YAML structure,
    /// known fields) before attempting conversion to the runtime model.
    fn validate(&self) -> Result<(), Self::Error>;

    /// Transforms the input configuration into the canonical `RuntimeConfig`.
    ///
    /// This step usually involves mapping fields, applying defaults, and converting types.
    fn build(self) -> Result<RuntimeConfig, Self::Error>;
}
