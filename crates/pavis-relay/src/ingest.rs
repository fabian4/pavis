use crate::config::{IngestSource, PipelineConfig};
use pavis_ingest_file::FileIngest;
use std::time::Duration;

pub enum IngestImpl {
    File(FileIngest),
}

pub fn create_ingest(config: &PipelineConfig) -> anyhow::Result<Option<IngestImpl>> {
    match &config.ingest.source {
        IngestSource::File(file_config) => {
            let debounce_ms = config.ingest.mode.debounce;
            Ok(Some(IngestImpl::File(FileIngest::new(
                &file_config.path,
                Duration::from_millis(debounce_ms),
            ))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_ingest_creates_file_ingest() {
        let config = PipelineConfig::default();
        let ingest = create_ingest(&config).expect("create ingest");
        assert!(matches!(ingest, Some(IngestImpl::File(_))));
    }
}
