//! Configuration types for Pavis proxy (Codec Layer).
//!
//! These types are used for parsing serde-backed configuration and validating it
//! before structural completion and conversion into `pavis_core::RuntimeConfig`.
//! Core semantic validation happens in the codec pipeline.

pub mod convert;
pub mod types;
pub mod validation;

#[allow(unused_imports)]
pub use convert::*;
pub use pavis_core::{AccessLogPolicy, HttpVersion, LoadBalancer, PathMatch};
pub use types::*;
