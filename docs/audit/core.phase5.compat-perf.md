# Phase 5 — Compatibility & Performance Signals Audit (crates/pavis-core)

## 1. Summary

**Signals Overview:**
*   **Compatibility:** **High Risk**. The public API relies heavily on structs with public fields and exhaustive enums without `#[non_exhaustive]`. This strictly limits the ability to add configuration options or enum variants without introducing breaking changes for downstream consumers.
*   **Performance:** **Moderate Risk**. While validation logic is generally efficient (linear time), a specific hotspot in route validation performs unnecessary string allocations for every route, which could scale poorly for large routing tables.

## 2. Compatibility Findings

### Public API Stability Surface
The crate exposes a large surface of "Canonical Types" in `src/runtime/**/*.rs`. These are the primary contract for configuration.

| Type | Location | Issue |
| :--- | :--- | :--- |
| `RuntimeConfig`, `Listener`, `Upstream` | `src/runtime.rs` | Structs use **public fields**. Adding a field (e.g., `Listener::keepalive`) will break all construction sites using struct literals. |
| `LoadBalancer`, `HttpVersion` | `src/runtime/upstream.rs` | Enums are **exhaustive**. Adding a variant (e.g., `HttpVersion::H3`) will break all downstream `match` expressions. |
| `Route`, `VirtualHost` | `src/runtime/routing.rs` | Public fields. |

### Evolution Risk Notes
*   **Missing `#[non_exhaustive]`**: None of the public configuration types use this attribute. This implies that the configuration schema is considered "frozen" or that the project accepts frequent major version bumps.
*   **Explicit Strategy**: No versioning constants or migration traits were found in the codebase.

## 3. Performance Signals

### Allocation/Copy Signals

| Location | Signal | Description | Severity |
| :--- | :--- | :--- | :--- |
| `src/validate/routes.rs` | `path.0.clone()` | **Unnecessary Allocation**. Inside the `validate_routes` loop, the path string is cloned *for every route* just to insert it into a temporary `HashSet` for duplicate detection. The set could instead store `&str` references, as the source `routes` slice outlives the set. | **Hotspot** (for large configs) |
| `src/validate/routes.rs` | `HashSet::new()` | **Repeated Allocation**. A new `HashSet` is allocated for *every* virtual host. For configs with thousands of hosts, this results in thousands of allocations. Reusing/clearing a single set buffer would be more efficient. | Cold |

### Complexity Signals

*   **Regex Compilation**: `validate_routes` compiles every regex route. This is O(R * M) where R is routes and M is regex complexity. While necessary for validation, it is a CPU-intensive step. Users with thousands of regex routes will experience slow startup/reload times.
*   **Route-Destination Check**: The validation of upstream destinations is O(U + R*D), which is efficient. `upstream_names` is built once per validation pass.

### Data Structure Signals

*   **Config Size**: The configuration hierarchy is a deep tree of `Vec<T>`. Serialization/Deserialization (even with `rkyv`) involves traversing this entire structure.

## 4. Bench Plan

The following benchmarks are recommended to establish baselines and detect regressions in validation performance.

| Bench Name | Target | Input Shape | Metric Intent | Threshold |
| :--- | :--- | :--- | :--- | :--- |
| `bench_validate_large_routes` | `validate_routes` | 1 VHost, 5000 Routes (Prefix) | **Allocations** & Latency. Detects cost of `path.clone()`. | >10% alloc increase |
| `bench_validate_regex_compile` | `validate_routes` | 1 VHost, 500 Regex Routes | **Latency**. Measures CPU cost of regex compilation loop. | >10% latency increase |
| `bench_validate_many_vhosts` | `validate_routes` | 2000 VHosts, 1 Route each | **Allocations**. Detects cost of repeated `HashSet::new()`. | >10% alloc increase |
| `bench_validate_upstreams` | `validate_upstreams` | 2000 Upstreams | **Latency**. Measures O(N) uniqueness check overhead. | >5% latency increase |
