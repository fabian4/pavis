use pavis_core::ValidatedRuntimeConfig;

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("single source authority violation: {0}")]
    SingleSource(String),
    #[error("version monotonicity violation: current={current}, proposed={proposed}")]
    VersionMonotonicity { current: u64, proposed: u64 },
    #[error("cache error: {0}")]
    Cache(String),
    #[error("storage error: {0}")]
    Storage(#[from] std::io::Error),
    #[error("policy enforcement error: {0}")]
    Policy(String),
}

pub fn execute_plan(
    current_version: u64,
    proposed_version: u64,
    config: ValidatedRuntimeConfig,
) -> Result<ValidatedRuntimeConfig, RelayError> {
    if proposed_version <= current_version {
        return Err(RelayError::VersionMonotonicity {
            current: current_version,
            proposed: proposed_version,
        });
    }

    let _ = config;
    Ok(config)
}
