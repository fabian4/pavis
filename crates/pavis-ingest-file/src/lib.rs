use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::Stream;
use notify::RecommendedWatcher;
use pavis_ingest_api::{Artifact, Format, Ingest, IngestError, SourceInfo};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context as StdContext, Poll};
use std::time::Duration;
use tokio::sync::mpsc;

mod watch;

pub use watch::spawn_watcher;

pub fn infer_format(path: &Path) -> Format {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("yaml") | Some("yml") => Format::Yaml,
        Some("json") => Format::Json,
        _ => Format::Unknown,
    }
}

pub(crate) fn is_supported(format: Format) -> bool {
    matches!(format, Format::Yaml | Format::Json)
}

pub struct FileIngest {
    path: PathBuf,
    debounce_duration: Duration,
}

impl FileIngest {
    pub fn new(path: impl Into<PathBuf>, debounce_duration: Duration) -> Self {
        Self {
            path: path.into(),
            debounce_duration,
        }
    }

    async fn read_artifact(&self) -> Result<Artifact, IngestError> {
        let bytes = tokio::fs::read(&self.path)
            .await
            .map_err(|e| IngestError::Io(anyhow::anyhow!(e)))?;

        let format = infer_format(&self.path);
        if !is_supported(format) {
            return Err(IngestError::Io(anyhow::anyhow!(
                "Unsupported file format for path: {:?}",
                self.path
            )));
        }

        let source = SourceInfo::new(self.path.to_string_lossy());
        Ok(Artifact::new(Bytes::from(bytes), format, source))
    }
}

pub struct FileIngestStream {
    receiver: mpsc::Receiver<Result<Artifact, IngestError>>,
    _watcher: RecommendedWatcher,
}

impl Stream for FileIngestStream {
    type Item = Result<Artifact, IngestError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut StdContext<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

#[async_trait]
impl Ingest for FileIngest {
    type Stream = FileIngestStream;

    async fn stream(&mut self) -> Result<Self::Stream, IngestError> {
        let (tx, rx) = mpsc::channel(1);

        // Emit initial state
        if let Ok(art) = self.read_artifact().await {
            let _ = tx.send(Ok(art)).await;
        }

        let watcher = watch::spawn_watcher(self.path.clone(), self.debounce_duration, tx).await?;

        Ok(FileIngestStream {
            receiver: rx,
            _watcher: watcher,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_infer_format() {
        assert_eq!(infer_format(Path::new("test.yaml")), Format::Yaml);
        assert_eq!(infer_format(Path::new("test.yml")), Format::Yaml);
        assert_eq!(infer_format(Path::new("test.json")), Format::Json);
        assert_eq!(infer_format(Path::new("test.txt")), Format::Unknown);
        assert_eq!(infer_format(Path::new("test")), Format::Unknown);
    }

    #[test]
    fn test_is_supported() {
        assert!(is_supported(Format::Yaml));
        assert!(is_supported(Format::Json));
        assert!(!is_supported(Format::Unknown));
        assert!(!is_supported(Format::XdsDelta));
    }

    #[tokio::test]
    async fn test_file_ingest_initial_load() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        file.as_file_mut().write_all(b"initial content")?;
        let path = file.path().to_path_buf();

        // Rename to have supported extension
        let yaml_path = path.with_extension("yaml");
        std::fs::rename(&path, &yaml_path)?;

        let mut ingest = FileIngest::new(yaml_path.clone(), Duration::from_millis(10));
        let mut stream = ingest.stream().await.map_err(|e| anyhow::anyhow!(e))?;

        if let Some(Ok(artifact)) = stream.next().await {
            assert_eq!(artifact.bytes, Bytes::from_static(b"initial content"));
            assert_eq!(artifact.format, Format::Yaml);
            assert_eq!(artifact.source.name, yaml_path.to_string_lossy());
        } else {
            panic!("Expected artifact");
        }

        std::fs::remove_file(yaml_path)?;
        Ok(())
    }

    #[tokio::test]
    async fn test_file_ingest_update() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        file.as_file_mut().write_all(b"v1")?;
        let path = file.path().to_path_buf();
        let yaml_path = path.with_extension("yaml");
        std::fs::rename(&path, &yaml_path)?;

        let mut ingest = FileIngest::new(yaml_path.clone(), Duration::from_millis(50));
        let mut stream = ingest.stream().await.map_err(|e| anyhow::anyhow!(e))?;

        // Skip initial
        let _ = stream.next().await;

        // Write update
        tokio::time::sleep(Duration::from_millis(10)).await;
        std::fs::write(&yaml_path, b"v2")?;

        if let Some(Ok(artifact)) = stream.next().await {
            assert_eq!(artifact.bytes, Bytes::from_static(b"v2"));
        } else {
            panic!("Expected updated artifact");
        }

        std::fs::remove_file(yaml_path)?;
        Ok(())
    }

    #[tokio::test]
    async fn test_file_ingest_rapid_updates_debounce() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        file.as_file_mut().write_all(b"v0")?;
        let path = file.path().to_path_buf();
        let yaml_path = path.with_extension("yaml");
        std::fs::rename(&path, &yaml_path)?;

        let mut ingest = FileIngest::new(yaml_path.clone(), Duration::from_millis(100));
        let mut stream = ingest.stream().await.map_err(|e| anyhow::anyhow!(e))?;

        // Skip initial
        let _ = stream.next().await;

        // Multiple rapid writes
        for i in 1..=5 {
            std::fs::write(&yaml_path, format!("v{i}").as_bytes())?;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        // We should get exactly one update after debounce
        if let Some(Ok(artifact)) = stream.next().await {
            assert_eq!(artifact.bytes, Bytes::from_static(b"v5"));
        } else {
            panic!("Expected updated artifact after debounce");
        }

        // Ensure no extra artifacts are waiting
        tokio::select! {
            _ = stream.next() => panic!("Unexpected additional artifact"),
            _ = tokio::time::sleep(Duration::from_millis(150)) => {}
        }

        std::fs::remove_file(yaml_path)?;
        Ok(())
    }

    #[tokio::test]
    async fn test_file_ingest_missing_initial_file_fails() -> Result<()> {
        let path = PathBuf::from("non_existent_file.yaml");
        let mut ingest = FileIngest::new(path, Duration::from_millis(10));

        // stream() should fail if file is missing because notify needs the path to exist
        let res = ingest.stream().await;
        assert!(res.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_file_ingest_empty_initial_file_then_update() -> Result<()> {
        let file = NamedTempFile::new()?;
        // Empty initial file
        let path = file.path().to_path_buf();
        let yaml_path = path.with_extension("yaml");
        std::fs::rename(&path, &yaml_path)?;

        let mut ingest = FileIngest::new(yaml_path.clone(), Duration::from_millis(50));
        let mut stream = ingest.stream().await.map_err(|e| anyhow::anyhow!(e))?;

        // Expect initial empty artifact
        if let Some(Ok(artifact)) = stream.next().await {
            assert!(artifact.bytes.is_empty());
        } else {
            panic!("Expected initial empty artifact");
        }

        // Write update
        tokio::time::sleep(Duration::from_millis(10)).await;
        std::fs::write(&yaml_path, b"now has content")?;

        if let Some(Ok(artifact)) = stream.next().await {
            assert_eq!(artifact.bytes, Bytes::from_static(b"now has content"));
        } else {
            panic!("Expected updated artifact");
        }

        std::fs::remove_file(yaml_path)?;
        Ok(())
    }

    #[tokio::test]
    async fn test_file_ingest_unsupported_format() -> Result<()> {
        let file = NamedTempFile::new()?;
        let path = file.path().to_path_buf();
        let txt_path = path.with_extension("txt");
        std::fs::rename(&path, &txt_path)?;

        let mut ingest = FileIngest::new(txt_path.clone(), Duration::from_millis(10));
        let mut stream = ingest.stream().await.map_err(|e| anyhow::anyhow!(e))?;

        // Should not emit anything for unsupported format
        tokio::select! {
            _ = stream.next() => panic!("Unexpected artifact for unsupported format"),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }

        std::fs::remove_file(txt_path)?;
        Ok(())
    }
}
