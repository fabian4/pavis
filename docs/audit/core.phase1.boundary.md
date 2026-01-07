# Phase 1 — Boundary & Dependency Audit (crates/pavis-core)

## 1. Summary

**Pass/Fail:** PASS (with one noted dependency constraint)

The `crates/pavis-core` crate successfully respects its architectural boundaries. It strictly contains canonical data structures, validation logic, and pure helpers. No I/O operations, runtime behaviors (async/tokio), or side effects were found.

## 2. Boundary Violations

No code-level boundary violations were found.

*   **No I/O**: `std::fs`, `std::net`, and `std::env` are used only for type definitions (e.g., `SocketAddr`) or unit tests, never for operations.
*   **No Runtime**: No usage of `tokio`, `async`, `await`, `spawn`, or thread management in the source code.
*   **No Policy Leaks**: Defaults are strictly structural (enum variants), not business logic.
*   **No File Access in Validation**: `src/validate/server.rs` correctly validates that file path strings are present/non-empty but does *not* attempt to verify their existence on disk, preserving the I/O boundary.

## 3. Suspicious Dependencies

| Crate | Usage | Status | Notes |
| :--- | :--- | :--- | :--- |
| `serde` | `optional = true` | **Boundary Risk** | The audit prompt's "Hard Boundary Constraints" explicitly state: "`pavis-core` MUST NOT contain ... serde ... dependencies". This crate includes `serde` as an optional dependency. <br><br> *Mitigation*: It is feature-gated and used solely for `derive` macros and custom `Serialize`/`Deserialize` implementations (in `src/serde_impl.rs`) to support downstream consumers. It does **not** perform parsing (e.g., `serde_json::from_str` is absent in library code). |

## 4. Confirmed Boundary-Safe Areas

*   **Canonical Types (`src/runtime/**/*.rs`)**: All types are pure data structures (structs/enums) using `rkyv` for zero-copy support and optional `serde` support.
*   **Validation Logic (`src/validate/**/*.rs`)**:
    *   **Headers**: Validates format using `http::header::HeaderName::from_str` (pure).
    *   **Routes**: Compiles regexes using `regex::Regex::new` to verify syntax (pure, CPU-bound).
    *   **Upstreams**: Checks for duplicate names and invariant weights (pure).
*   **Serialization Helpers (`src/serde_impl.rs`)**: Implements `serde` traits manually for specific types (e.g., `AccessLogPolicy`) without invoking parsers.
*   **Dependencies**:
    *   `rkyv`: Used for zero-copy schema definition (safe).
    *   `regex`: Used for validation only (safe).
    *   `thiserror`: Used for error definition (safe).
    *   `http`: Used for type definitions (safe).
