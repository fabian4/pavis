# Core Audit Summary (crates/pavis-core)

## 1. Scope & Method Recap
This audit covered the `crates/pavis-core` crate, focusing on its role as the canonical type and validation library for the Pavis system.
**Phases Completed:** Inventory (0), Boundary (1), Invariants (2), Errors (3), Safety (4), Compatibility & Performance (5).
**Out of Scope:** Runtime behavior (networking, async), downstream crates (`pavis-pvs`, `pavis-relay`), and integration tests outside this crate.

## 2. Overall Assessment
**Verdict:** **Core is mostly sound but requires fixes before lock-in.**

The crate excels in safety and boundary enforcement. It is panic-free, type-safe, and free of I/O or runtime pollution. However, it faces **significant compatibility risks** due to a lack of evolution strategies (public fields, exhaustive enums) and has one notable performance hotspot in its validation logic.

## 3. Cross-Phase Risk Synthesis

| Risk | Origin Phase(s) | Impact |
| :--- | :--- | :--- |
| **Brittle API Evolution** | Phase 5 (Compat) | **Critical**. Public structs and exhaustive enums define a "frozen" API. Any addition of fields or variants will be a breaking change for all downstream consumers, making iterative development painful. |
| **Validation Allocations** | Phase 5 (Perf) | **High**. The route validation logic clones string paths for *every* route entry to perform duplicate detection. This creates unnecessary allocator pressure scaling linearly with config size. |
| **Duplicate VirtualHosts** | Phase 2 (Invariants) | **Medium**. The validation logic checks for duplicate routes *within* a host but allows multiple `VirtualHost` entries for the same domain (e.g., two blocks for `example.com`), leading to undefined runtime behavior. |
| **Generic TLS Error** | Phase 3 (Errors) | **Low**. The `MissingTlsFiles` error lacks context (Listener Name), making it difficult to debug configurations with multiple listeners. |

## 4. Readiness Assessment

*   **Invariants Enforced?** **MOSTLY**. Strong type-level guarantees (`NonZeroU16`), but missing uniqueness checks for VirtualHosts and Listeners.
*   **Diagnosable Error Model?** **YES**. Structured `thiserror` types are used consistently, with one minor context gap.
*   **Safety & Panic Risks?** **YES**. The crate is exceptionally safe, with zero production panics or unsafe memory operations.
*   **Compat & Perf Bounded?** **NO**. Compatibility is the weakest area; performance has a clear optimization target.

## 5. Recommended Next Actions

1.  **Blocker**: Add `#[non_exhaustive]` to all public enums (`LoadBalancer`, `HttpVersion`, etc.) and consider builder patterns or private fields for structs to enable future expansion.
2.  **Fix**: Rewrite `validate_routes` to store `&str` references in the duplicate detection set, eliminating the `path.clone()` hotspot.
3.  **Fix**: Add validation logic to enforce uniqueness of `VirtualHost` domains and `Listener` names.
4.  **Proceed**: Begin the audit of `crates/pavis-pvs` (Phase 0), as the core data structures are semantically stable enough to review their serialization format.

## 6. Relationship to Downstream Crates

*   **pavis-pvs**: relies on `pavis-core` types for schema definition. The "Brittle API" risk means changes here will force major version bumps in the PVS format.
*   **runtime**: relies on `ValidatedRuntimeConfig` for safety. The "Duplicate VirtualHost" gap means the runtime currently must handle (or mishandle) colliding host definitions.
