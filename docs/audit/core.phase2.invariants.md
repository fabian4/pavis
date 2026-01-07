# Phase 2 — Invariants & Correctness Audit (crates/pavis-core)

## 1. Invariant Inventory

| Invariant Name | Applies To | Description | Enforcement |
| :--- | :--- | :--- | :--- |
| **Valid Construction** | `ValidatedRuntimeConfig` | Ensures config has passed validation before use. | **Partial**. `new` is crate-private, but `assume_validated` and `from_trusted` allow bypass. |
| **Upstream Name Uniqueness** | `RuntimeConfig.upstreams` | All upstreams must have unique names. | `validate_upstreams` |
| **Upstream Name Non-Empty** | `UpstreamName` | Upstream names cannot be empty strings. | `validate_upstreams` |
| **Positive Weight** | `Weight` | Weights for endpoints and destinations must be > 0. | **Type System** (`NonZeroU16`). |
| **Destination Validity** | `RouteAction::Forward` | All upstream references must exist. | `validate_routes` |
| **Route Uniqueness** | `VirtualHost` | No duplicate (Match Type, Path) pairs within a host. | `validate_routes` |
| **Path Normalization** | `Path` | Paths must start with `/` and not end with `/` (unless root), except Regex. | `validate_routes` |
| **Regex Validity** | `PathMatch::Regex` | Regex patterns must compile successfully. | `validate_routes` |
| **Regex Constraints** | `PathMatch::Regex` | Regex length ≤ 2048; Rewrites disabled for regex routes. | `validate_routes` |
| **Forward Action Validity** | `RouteAction::Forward` | Must have at least one destination. | `validate_routes` |
| **Header Validity** | `Headers` | Header names/values must satisfy HTTP spec. | `validate_headers` |
| **TLS Config Integrity** | `TlsConfig` | Certificate and Key paths must be non-empty if TLS is enabled. | `validate_server` |

## 2. Invariant Enforcement Matrix

All enforced invariants return `CoreValidationError` on failure.

| Invariant | Enforced Location | Complete? | Bypass Risk |
| :--- | :--- | :--- | :--- |
| **Valid Construction** | `src/runtime.rs` | Yes | **High**: `assume_validated` exists for optimization/trust. |
| **Upstream Uniqueness** | `src/validate/upstreams.rs` | Yes | No |
| **Positive Weight** | `src/runtime/types.rs` | Yes | No (Type System guarantees) |
| **Destination Validity** | `src/validate/routes.rs` | Yes | No |
| **Route Uniqueness** | `src/validate/routes.rs` | Yes | No |
| **Path Normalization** | `src/validate/routes.rs` | Yes | No |
| **Regex Validity** | `src/validate/routes.rs` | Yes | No |
| **TLS Config Integrity** | `src/validate/server.rs` | Yes | No |

## 3. Correctness Gaps

These are conditions that *should* likely be invariants but are currently unenforced.

1.  **Virtual Host Uniqueness**: `validate_routes` iterates over `routes` (VirtualHosts) but does **not** check if multiple entries define the same `host` (e.g., two blocks for `example.com`). This could lead to undefined behavior or silent shadowing in the runtime.
2.  **Listener Name Uniqueness**: `validate_runtime` iterates listeners but ignores `name`. Duplicate listener names could cause metrics collisions or administrative confusion.
3.  **Listener Address Uniqueness**: `validate_runtime` does not check for duplicate bind addresses. While the OS will catch this at runtime, a config validation error would be cleaner.
4.  **Telemetry Validation**: `Telemetry` struct fields (like `SampleRate`) are never validated. A `SampleRate` of `999999` (if interpreted as percentage or ratio) might be invalid but is currently accepted.

## 4. Missing or Weak Tests

*   **Positive Weight**: The test `endpoint_weight_zero_fails` exists in `src/validate.rs` but is a placeholder comment. It should specifically attempt to deserialize a config with weight 0 (via JSON/YAML) to demonstrate that the type system or deserializer rejects it.
*   **Virtual Host Duplication**: No test exists (because it's not enforced), but a test *should* exist to demonstrate the current behavior (acceptance) until the gap is fixed.
