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

#[cfg(test)]
mod tests {
    use super::{Config, ConfigSource};
    use crate::RuntimeConfig;
    use std::error::Error;
    use std::fmt;

    #[derive(Debug)]
    struct DummyError(&'static str);

    impl fmt::Display for DummyError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Error for DummyError {}

    #[derive(Debug, Clone)]
    struct DummyConfig(String);

    impl Config for DummyConfig {
        type Error = DummyError;

        fn load(source: ConfigSource) -> Result<Self, Self::Error> {
            match source {
                ConfigSource::String(s) => Ok(Self(s.to_string())),
                ConfigSource::Bytes(bytes) => match std::str::from_utf8(bytes) {
                    Ok(s) => Ok(Self(s.to_string())),
                    Err(_) => Err(DummyError("invalid utf8")),
                },
                ConfigSource::File(_) => Err(DummyError("file not supported in test")),
            }
        }

        fn validate(&self) -> Result<(), Self::Error> {
            if self.0.is_empty() {
                return Err(DummyError("empty"));
            }
            Ok(())
        }

        fn build(self) -> Result<RuntimeConfig, Self::Error> {
            Err(DummyError("no runtime in test"))
        }
    }

    #[test]
    fn load_from_string_and_bytes() {
        let cfg = DummyConfig::load(ConfigSource::String("ok")).expect("string load");
        assert_eq!(cfg.0, "ok");

        let cfg = DummyConfig::load(ConfigSource::Bytes(b"ok")).expect("bytes load");
        assert_eq!(cfg.0, "ok");
    }

    #[test]
    fn validate_rejects_empty() {
        let cfg = DummyConfig(String::new());
        let err = cfg.validate().expect_err("empty should fail");
        assert_eq!(err.to_string(), "empty");
    }
}
