Audit Phase: Core Audit
Target Crate: crates/pavis-core
Generation Timestamp: 2026-01-14T12:00:00Z
AI Model: Gemini

# 1. Executive Verdict

**Verdict:** Sound

The `pavis-core` crate is a robust, pure-data library that successfully isolates configuration definitions and validation logic from runtime effects. It exhibits zero I/O, threading, or async behavior, strictly adhering to the architectural boundary requirements. The validation logic is comprehensive, enforcing semantic invariants (uniqueness, referential integrity, format correctness) beyond simple type safety. While there are minor API design risks regarding evolution (rigidity) and a confusing safety contract in the `ValidatedRuntimeConfig` wrapper, the codebase is free of Undefined Behavior (UB) risks and is structurally ready for production lock-in.

# 2. Top System Risks

1.  **Ambiguous Validation Contract (Phase 2):**
    The `ValidatedRuntimeConfig` type, intended to serve as a witness of validity, exposes two constructor methods with identical implementations but conflicting safety contracts:
    -   `pub unsafe fn from_trusted` (implies validation is a safety invariant)
    -   `pub fn assume_validated` (implies validation is optional/advisory)
    This allows safe code to trivially bypass validation, undermining the type's guarantee without an `unsafe` block.

2.  **API Rigidity (Phase 5):**
    The primary configuration structures (e.g., `RuntimeConfig`, `Listener`) expose all fields as `pub`. While flexible for serialization, this prevents non-breaking addition of fields in the future (breaking struct literal construction) because `#[non_exhaustive]` is applied inconsistently (used on Enums, but not on top-level Structs).

3.  **Regex Compilation Overhead (Phase 5):**
    The `validate_routes` function compiles regexes (`Regex::new`) inside a loop for every validation pass. While the configuration size is likely bounded in practice, this represents a linear performance cost relative to the number of regex routes, which could be significant for very large configurations.

# 3. Readiness Assessment

| Criteria | Status | Notes |
| :--- | :--- | :--- |
| **Invariants Enforced?** | **Yes** | enforced via `validate_runtime` (referential integrity, uniqueness, formats) and types (`NonZeroU16`). |
| **Diagnosable Errors?** | **Yes** | `CoreValidationError` provides specific context (field names, values) for all failure modes. |
| **Safety Acceptable?** | **Yes** | No Undefined Behavior risks found. `unsafe` is used correctly for trusted deserialization, though the safe bypass (`assume_validated`) is a logical flaw. |
| **API Compatibility?** | **Yes** | `rkyv` guarantees layout. `non_exhaustive` on enums aids evolution. Public fields on structs are a known rigidity trade-off. |

# 4. Recommended Next Steps

1.  **Clarify Validation Contract:** Deprecate or remove `assume_validated` in favor of `unsafe fn from_trusted` to enforce the "validation as a contract" model, or document clearly that validation is for logical correctness only, not memory safety.
2.  **Adopt Builder Pattern:** Introduce builder types or constructor functions for `RuntimeConfig` and its children to mitigate the breaking change risk of adding new configuration fields.
3.  **Optimize Regex Validation:** Consider using `lazy_static` or a compilation cache if validation performance becomes a bottleneck, though the current "validate on load" model is acceptable for Phase 1.
4.  **Verify Edition:** Confirm the intent of `edition = "2024"` in `Cargo.toml`. While likely a placeholder for the next edition, it should be verified against the project's compiler support matrix.

# 5. Detailed Analysis

## Phase 0: Inventory & API Surface
-   **Structure:** Clean separation between `runtime` (types) and `validate` (logic).
-   **Dependencies:** Minimal and appropriate (`rkyv`, `serde`, `regex`, `thiserror`, `http`).
-   **API:** Exports `RuntimeConfig`, `ValidatedRuntimeConfig`, and validation functions.

## Phase 1: Boundary & Dependency Audit
-   **I/O & Side Effects:** **PASSED.** No `std::fs`, `std::net` (except types), or async usage found.
-   **Dependencies:** All dependencies are used for data manipulation or validation only. `validate_server` checks certificate paths are non-empty strings but strictly avoids filesystem checks, correctly pushing I/O to the runtime layer.

## Phase 2: Invariants & Correctness
-   **Enforcement:** Strong enforcement of semantic invariants:
    -   *Uniqueness:* Listener names, Upstream names, VirtualHost domains.
    -   *Referential Integrity:* Routes must reference existing upstreams (`UnknownDestination`).
    -   *Constraints:* `verify=full` requires SNI; Regex routes cannot use rewrites.
-   **Gap:** The `assume_validated` bypass mentioned in Risks.
    ```rust
    // crates/pavis-core/src/runtime.rs
    pub fn assume_validated(runtime: RuntimeConfig) -> Self { Self { runtime } }
    ```

## Phase 3: Error Model & Diagnostics
-   **Quality:** Errors are strongly typed and context-rich.
    ```rust
    // crates/pavis-core/src/validate.rs
    #[error("route '{0}' (host '{1}') references unknown upstream '{2}'")]
    UnknownDestination(String, String, String),
    ```
-   **Panic Policy:** No usage of `unwrap`, `expect`, or `panic!` found in library code. `Regex::new` and `HeaderName::from_str` failures are correctly mapped to `CoreValidationError`.

## Phase 4: Safety & Undefined Behavior
-   **Unsafe Code:** Limited to `from_trusted` (justified) and test helpers.
    ```rust
    // crates/pavis-core/src/runtime.rs
    pub unsafe fn from_trusted(runtime: RuntimeConfig) -> Self { Self { runtime } }
    ```
-   **Memory Safety:** Relies on `rkyv` for zero-copy safety. `#[archive(check_bytes)]` is correctly applied to all `Archive` types, ensuring deserialization validates memory layout.

## Phase 5: Compatibility & Performance
-   **Evolution:** `#[non_exhaustive]` is used on all Enums (`TlsConfig`, `LoadBalancer`, etc.), which is excellent. Structs rely on public fields.
-   **Performance:**
    -   Validation iterates collections and creates `HashSet`s. Complexity is generally linear $O(N)$.
    -   Regex compilation is repeated per validation.
    ```rust
    // crates/pavis-core/src/validate/routes.rs
    let _compiled = Regex::new(&path.0).map_err(...)
    ```
    -   Strings are cloned into Errors, which is acceptable for the failure path.