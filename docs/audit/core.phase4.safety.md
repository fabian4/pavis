# Phase 4 — Safety & Panic / Undefined Behavior Risk Audit (crates/pavis-core)

## 1. Summary

**Pass/Fail:** PASS

The `crates/pavis-core` crate is structurally safe. It avoids panics, unsafe memory manipulation, and interior mutability entirely in its production code. The single use of `unsafe` is a contract marker for skipping validation, which is appropriate. The use of strict types (`NonZeroU16`, `check_bytes`) ensures that invalid states cannot be constructed even via zero-copy deserialization.

## 2. Panic Risk Findings

**Zero** panic sources were identified in non-test code.

*   **No Unwraps**: Verified in Phase 3.
*   **No Indexing**: Iterators (`for x in &collection`) are used exclusively instead of manual indexing (`collection[i]`), preventing out-of-bounds panics.
*   **No Arithmetic**: The crate defines data structures and performs validation logic that does not involve complex arithmetic or potential overflows.

## 3. Unsafe Code Review

| Location | Symbol | Justification | Assessment |
| :--- | :--- | :--- | :--- |
| `src/runtime.rs` | `ValidatedRuntimeConfig::from_trusted` | Marks a constructor as "unsafe" because it allows bypassing validation logic. The "unsafety" here is semantic (violating invariants) rather than memory-unsafe. | **Safe / Justified**. Documented properly. |

*Observation*: The crate also exposes `assume_validated` which performs the exact same operation but is *safe* (relying on documentation). This redundancy is harmless but slightly inconsistent API design.

## 4. Input Assumption Risks

*   **Serialization Safety**: All types derive `rkyv::Archive` with `#[archive(check_bytes)]`. This ensures that even when deserializing from untrusted bytes, invariants like `NonZeroU16` (used for `Weight`) are enforced, preventing the creation of invalid values (e.g., 0 weight) that could cause division-by-zero downstream.
*   **Collection Safety**: Validation logic handles empty collections (listeners, upstreams) gracefully without panicking.

## 5. Memory & Lifetime Analysis

*   **Ownership**: All configuration types use owned data (`String`, `Vec<T>`). There are no lifetimes or references, eliminating use-after-free or dangling pointer risks.
*   **Aliasing**: No internal mutability (`RefCell`, `Mutex`) is used. Data is immutable once constructed (unless modified by the owner), ensuring thread safety by default.

## 6. Concurrency & Interior Mutability Review

*   **None**. The crate defines pure data structures (POD) and functions. It is agnostic to the concurrency model of the runtime.

## 7. Missing Tests for Safety-Critical Paths

*   **Deserialization Safety**: A test case specifically fuzzing `rkyv` deserialization with invalid bytes (e.g., all zeros for a `NonZeroU16` field) would confirm that `check_bytes` is correctly protecting the boundary. Currently, safety is inferred from the derive macro.
