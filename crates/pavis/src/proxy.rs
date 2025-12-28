//! Proxy module: The runtime coordination layer.
//!
//! # Architectural Invariants
//!
//! 1. **No Business Logic**: This module orchestrates `router`, `upstream`, and `telemetry`.
//!    It should not contain complex logic for matching or load balancing.
//! 2. **Non-Blocking**: All operations must be async and non-blocking.
//!    - No `std::sync::Mutex` (use `tokio::sync::Mutex` if absolutely necessary, but prefer lock-free).
//!    - No blocking I/O (file, network).
//! 3. **No Mutable Global State**: State should be encapsulated in components (`Router`, `Manager`).
//! 4. **Validated Configuration**: The proxy assumes configuration is valid and immutable.

pub mod context;
pub mod header_ops;
pub mod service;

pub use context::RouterContext;
pub use header_ops::{apply_request_headers, apply_response_headers};
pub use service::Proxy;
