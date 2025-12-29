use bytes::Bytes;
use std::collections::BTreeMap;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct Artifact {
    pub bytes: Bytes,
    pub format: Format,
    pub source: SourceInfo,
    pub version: Option<String>,
    pub etag: Option<String>,
    pub received_at: SystemTime,
    pub content_type: Option<String>,
}

impl Artifact {
    pub fn new(bytes: Bytes, format: Format, source: SourceInfo) -> Self {
        Self {
            bytes,
            format,
            source,
            version: None,
            etag: None,
            received_at: SystemTime::now(),
            content_type: None,
        }
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Yaml,
    Json,
    XdsDelta,
    XdsState,
    Crd,
    Unknown,
}

#[derive(Debug, Clone, Default)]
pub struct SourceInfo {
    pub name: String,
    pub labels: BTreeMap<String, String>,
}

impl SourceInfo {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            labels: BTreeMap::new(),
        }
    }

    pub fn unknown() -> Self {
        Self {
            name: "unknown".to_string(),
            labels: BTreeMap::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("I/O error: {0}")]
    Io(anyhow::Error),
    #[error("transport error: {0}")]
    Transport(anyhow::Error),
    #[error("auth error: {0}")]
    Auth(anyhow::Error),
    #[error("watch/stream error: {0}")]
    Watch(anyhow::Error),
    #[error("reconnect error: {0}")]
    Reconnect(anyhow::Error),
    #[error("backoff error: {0}")]
    Backoff(anyhow::Error),
    #[error("upstream api error: {0}")]
    Upstream(anyhow::Error),
}

impl From<anyhow::Error> for IngestError {
    fn from(err: anyhow::Error) -> Self {
        Self::Io(err)
    }
}

#[async_trait::async_trait]
pub trait Ingest {
    type Stream: futures_core::Stream<Item = Result<Artifact, IngestError>> + Send + Unpin;

    async fn stream(&mut self) -> Result<Self::Stream, IngestError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::time::SystemTime;

    #[test]
    fn artifact_constructors_populate_defaults() {
        let before = SystemTime::now();
        let source = SourceInfo::new("source-a");
        let artifact = Artifact::new(Bytes::from_static(b"data"), Format::Yaml, source.clone());
        let after = SystemTime::now();

        assert_eq!(artifact.format, Format::Yaml);
        assert_eq!(artifact.source.name, "source-a");
        assert!(artifact.version.is_none());
        assert!(artifact.etag.is_none());
        assert!(artifact.content_type.is_none());
        assert!(artifact.received_at >= before);
        assert!(artifact.received_at <= after);

        let artifact = artifact.with_content_type("application/yaml");
        assert_eq!(artifact.content_type.as_deref(), Some("application/yaml"));
        assert_eq!(artifact.source.name, "source-a");
    }

    #[test]
    fn source_info_defaults_are_consistent() {
        let source = SourceInfo::new("source-b");
        assert_eq!(source.name, "source-b");
        assert!(source.labels.is_empty());

        let source = SourceInfo::unknown();
        assert_eq!(source.name, "unknown");
        assert!(source.labels.is_empty());
    }

    #[test]
    fn ingest_error_from_anyhow_maps_to_io() {
        let err: IngestError = anyhow::anyhow!("boom").into();
        assert!(matches!(err, IngestError::Io(_)));
    }
}
