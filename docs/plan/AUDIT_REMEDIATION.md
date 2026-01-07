# Remediation Plan: Pavis Core, PVS & Runtime

This plan outlines the steps required to address the critical stability, safety, and performance issues identified across the Pavis ecosystem (`pavis-core`, `pavis-pvs`, `pavis`).

## 1. Core Stability & API Hardening (`crates/pavis-core`)
**Goal:** Prevent breaking changes for downstream consumers and eliminate validation bottlenecks.

*   [x] **1.1. Freeze Public API (Compatibility)**
    *   **Action:** Add `#[non_exhaustive]` to all public enums (e.g., `LoadBalancer`, `HttpVersion`) and relevant structs in `src/runtime/`.
    *   **Reason:** Allows adding new variants/fields in future releases without breaking SemVer guarantees for consumers like the codec and runtime.

*   [x] **1.2. Optimize Route Validation (Performance)**
    *   **Action:** Refactor `validate_routes` in `src/validate/routes.rs`.
    *   **Change:** Replace `HashSet<String>` with `HashSet<&str>` for duplicate detection.
    *   **Reason:** Removes unnecessary heap allocations that scale linearly with config size.

*   [x] **1.3. Enforce Configuration Uniqueness (Correctness)**
    *   **Action:** Update `validate_runtime` in `src/validate.rs`.
    *   **Change:** Add validation checks to enforce uniqueness for:
        *   `Listener` names.
        *   `VirtualHost` domains.
    *   **Reason:** Prevents undefined behavior at runtime where ambiguous configurations could be loaded.

*   [x] **1.4. Prepare for Zero-Copy Headers (Performance)**
    *   **Action:** Update `Route` struct in `src/runtime/routing.rs`.
    *   **Change:** Wrap `HeadersPolicy` in `std::sync::Arc`.
    *   **Reason:** Enables the runtime to clone a pointer instead of deep-cloning a vector of strings for every request.

## 2. PVS Protocol Safety (`crates/pavis-pvs`)
**Goal:** Ensure the binary configuration format is robust against corruption and attacks.

*   [ ] **2.1. Zero-Panic Header Parsing (Safety)**
    *   **Action:** Refactor `read::parse_header`.
    *   **Change:** Replace `unwrap()` on slice conversions with proper `PvsError` propagation.
    *   **Reason:** Eliminates potential crash vectors in the validation path.

*   [ ] **2.2. Enhance Error Diagnostics (DevEx)**
    *   **Action:** Update `PvsError` definition.
    *   **Change:** Add `expected` vs `found` context fields to `InvalidMagic` and `ChecksumMismatch` errors.
    *   **Reason:** Significantly speeds up debugging of corrupted config files.

*   [ ] **2.3. DoS Protection (Security)**
    *   **Action:** Update `verify_bytes`.
    *   **Change:** Enforce a `MAX_PAYLOAD_SIZE` constant (e.g., 100MB).
    *   **Reason:** Prevents memory exhaustion attacks via maliciously crafted large files.

*   [x] **2.4. Efficient Handle Cloning (Performance)**
    *   **Action:** Refactor `VerifiedPvs`.
    *   **Change:** Store the verified bytes in `Arc<VerifiedBytes>` instead of `Vec<u8>`.
    *   **Reason:** Avoids expensive deep copies when passing configuration handles between components (Relay -> Runtime).

## 3. Runtime Polish (`crates/pavis`)
**Goal:** Improve operational visibility and finalize performance optimizations.

*   [x] **3.1. Implement Zero-Copy Header Policy (Performance)**
    *   **Action:** Update `Proxy::request_filter` in `src/proxy/service.rs`.
    *   **Change:** Adapt logic to use the `Arc<HeadersPolicy>` introduced in Core.
    *   **Reason:** Resolves the primary hot-path allocation bottleneck.

*   [ ] **3.2. SNI Observability (Operations)**
    *   **Action:** Update `Proxy::upstream_peer` in `src/proxy/service.rs`.
    *   **Change:** Log a `tracing::warn!` when falling back to "localhost" SNI (if no SNI configured).
    *   **Reason:** Helps operators debug connection failures to upstreams that strictly enforce SNI.

## 4. Execution Order
1.  **Phase 1 (Core)**: Must happen first as it changes types consumed by PVS and Runtime.
2.  **Phase 2 (PVS)**: Can proceed in parallel with Runtime changes once Core types are stable.
3.  **Phase 3 (Runtime)**: Dependent on Core changes for the zero-copy header optimization.
