#[derive(Debug, Clone, Default)]
pub struct RelayOptions {
    pub enable_file_ingest: bool,
    pub ingest_debounce_ms: u64,
}