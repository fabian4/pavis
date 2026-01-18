# Architectural Decision Records (ADR)

This document tracks significant architectural decisions that shape the Pavis project.

## ADR-007: TLS Termination Strategy

*   **Status**: Accepted
*   **Date**: 2026-01-06
*   **Problem**: L7 inspection requires decryption, but handling keys adds complexity to the frozen runtime.
*   **Decision**: Support inbound Server-side TLS Termination using file-based keys.
*   **Trade-off**:
    *   **Pros**: Enables L7 routing and header manipulation.
    *   **Cons**: Increases runtime startup checks (files must exist).
    *   **Mitigation**: The Runtime validates file existence before binding the listener.

## ADR-008: Action & Rewrite Primitives

*   **Status**: Accepted
*   **Date**: 2026-01-06
*   **Problem**: How to enable traffic modification without introducing Turing-complete scripting?
*   **Decision**: Implement a fixed set of atomic actions:
    1.  `Redirect` (3xx)
    2.  `DirectResponse` (Synthetic 200/400/503)
    3.  `Rewrite` (Prefix & Host only)
*   **Trade-off**:
    *   **Pros**: All actions are validated at compile time (Codec). The runtime executes them as simple data transformations.
    *   **Cons**: Less flexible than Lua/WASM. Complex logic requires an upstream service.

## ADR-009: TLS Backend Selection

*   **Status**: Accepted
*   **Date**: 2026-01-18
*   **Problem**: Pingora's default `rustls` backend lacks features required for Enterprise mesh environments (inbound mTLS, custom CAs).
*   **Decision**: Support both `rustls` (default) and `OpenSSL` (feature-gated) backends.
*   **Implication**:
    *   **Feature Parity**: Features dependent on OpenSSL (mTLS) are only available in builds with that feature enabled.
    *   **Config Validation**: The Codec validates configuration against the *target* backend capabilities (or warns if backend is unknown at compile time).
    *   **See Also**: `docs/operations/known-limits.md` for specific limitation details.
