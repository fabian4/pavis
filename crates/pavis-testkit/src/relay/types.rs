use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct ArtifactMeta {
    pub rev: u64,
    pub etag: String,
    pub size: usize,
    pub checksum: String,
}
