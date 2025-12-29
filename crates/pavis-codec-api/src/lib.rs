use pavis_core::{CoreValidationError, ValidatedRuntimeConfig};
use pavis_ingest_api::Artifact;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("check error: {0}")]
    Check(anyhow::Error),
    #[error("compile error: {0}")]
    Compile(anyhow::Error),
    #[error(transparent)]
    Core(#[from] CoreValidationError),
}

/// Artifact that passed codec-level checks (syntax/schema/version gates).
#[derive(Debug, Clone)]
pub struct CheckedArtifact(pub Artifact);

pub trait Codec {
    type Error: std::error::Error + Send + Sync + 'static;

    fn check(&self, art: Artifact) -> Result<CheckedArtifact, Self::Error>;

    fn compile(&self, checked: &CheckedArtifact) -> Result<pavis_core::RuntimeConfig, Self::Error>;

    fn decompile(&self, cfg: &pavis_core::RuntimeConfig) -> Result<Artifact, Self::Error>;

    fn materialize(&self, art: Artifact) -> Result<ValidatedRuntimeConfig, Self::Error>
    where
        Self::Error: From<CoreValidationError>,
    {
        let checked = self.check(art)?;
        let cfg = self.compile(&checked)?;
        pavis_core::validate_runtime(cfg).map_err(Self::Error::from)
    }
}
