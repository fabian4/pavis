# Phase 3 — Error Model & Diagnostics Audit (crates/pavis-core)

## 1. Summary

**Pass/Fail:** PASS

The `crates/pavis-core` crate demonstrates a robust and safe error model. It strictly separates validation logic from runtime behavior, using `thiserror` to define structured, context-rich errors.
Crucially, there are **ZERO** usages of `unwrap`, `expect`, or `panic!` in production code paths, ensuring that the library will never crash the application due to configuration issues.

## 2. Error Inventory

| Type | Location | Purpose |
| :--- | :--- | :--- |
| `CoreValidationError` | `src/validate.rs` | The primary error enum for all configuration validation failures. Derives `thiserror::Error`. |
| `CoreValidationResult<T>` | `src/validate.rs` | Type alias for `Result<T, CoreValidationError>`. |

## 3. Error Semantics & Context Review

The `CoreValidationError` enum generally provides excellent diagnostic context, embedding specific field values, route paths, and hostnames to help users pinpoint errors in complex configurations.

**Findings:**

*   **High-Quality Context**: most variants (e.g., `UnknownDestination`, `DuplicateRoute`, `InvalidRegex`) include the `host`, `route`, or `upstream_name` involved.
*   **Diagnostics Gap (`MissingTlsFiles`)**: The `MissingTlsFiles` variant is a unit variant with no fields.
    *   *Evidence*: `src/validate/server.rs` returns this error without capturing the `ListenerName`.
    *   *Impact*: In a multi-listener configuration, a "tls enabled but cert_path/key_path missing" error gives no indication of *which* listener is misconfigured.

## 4. Error Propagation Findings

*   **Fail-Fast**: Validation functions use the `?` operator to short-circuit on the first error.
*   **Lossless**: Errors are propagated as structured types up to the public API boundary (`validate_runtime`).
*   **No Swallowing**: All distinct validation failures result in a returned error.

## 5. Panic / Unwrap / Expect Policy

*   **Production Code**: **Clean**. No instances of `unwrap`, `expect`, `panic!`, `todo!`, or `unimplemented!` were found in non-test code.
*   **Test Code**: `unwrap` and `expect` are used appropriately in test modules to assert success/failure.

## 6. Missing Tests for Error Paths

*   **Contextual Ambiguity**: While `missing_tls_files_fails` asserts that the error is returned, there is no test demonstrating the *ambiguity* of this error when multiple listeners are present. A test case configuring two listeners—one valid, one missing keys—would reveal the diagnostic gap.
