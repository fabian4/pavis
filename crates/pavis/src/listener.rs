//! Listener module encapsulates runtime-only listener concerns.
//!
//! Currently this focuses on TLS materialization so the proxy loop stays
//! agnostic of OpenSSL specifics. Future listener-side capabilities (ALPN,
//! OCSP stapling, etc.) should extend this module rather than reintroducing
//! ad-hoc logic inside `main.rs`.

pub mod tls;
