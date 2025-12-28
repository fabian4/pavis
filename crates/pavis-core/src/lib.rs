pub mod header;
pub mod runtime;
pub mod validate;

pub use header::{
    HEADER_SIZE, PAVIS_MAGIC, PAVIS_VERSION, PavisHeader, compute_checksum, format_address,
};
pub use runtime::*;
pub use validate::{
    CoreValidationError, CoreValidationResult, validate_runtime, validate_runtime_config,
};

#[cfg(feature = "serde")]
mod serde_impl;

#[cfg(test)]
mod tests;
