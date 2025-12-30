# Public API & Boundary Stability Review

## Active Findings (Latest)

- None.

## Historical Reviews

<!-- Append-only chronological log -->

### Review 2025-12-30T03:33:22Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-core`, `crates/pavis`

Summary:
- Made bypassing validation an explicit unsafe operation for trusted configurations.

Findings:
- [DONE] `ValidatedRuntimeConfig::from_trusted` is public and can bypass semantic validation
  - Evidence: `crates/pavis-core/src/runtime.rs` now exposes `pub unsafe fn from_trusted` with safety docs; `crates/pavis/src/load.rs` uses an `unsafe` block.
  - Resolution: Bypass requires explicit unsafe acknowledgment and documentation.

Resolved:
- `ValidatedRuntimeConfig::from_trusted` is public and can bypass semantic validation.

Notes:
- Timestamp (UTC): 2025-12-30T03:33:22Z
- Limitations: This change makes bypass explicit but does not remove the capability for trusted callers.

### Review 2025-12-30T03:25:01Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-core`

Summary:
- Confirmed the only remaining active public API concern is `ValidatedRuntimeConfig::from_trusted`.

Findings:
- [EXISTING] `ValidatedRuntimeConfig::from_trusted` is public and can bypass semantic validation
  - Evidence: `crates/pavis-core/src/runtime.rs` still exposes `pub fn from_trusted`.
  - Status: No change in this pass.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-30T03:25:01Z
- Limitations: Public API scan focused on core wrapper constructors only.

### Review 2025-12-30T03:18:52Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-core`, `crates/pavis`

Summary:
- Removed runtime-only regex state from the core public API surface.

Findings:
- [DONE] `Route::compiled_regex` exposes runtime-only regex state in the core public API
  - Evidence: `crates/pavis-core/src/runtime/routing.rs` no longer includes `compiled_regex`.
  - Resolution: Core route model contains only canonical fields; runtime keeps compiled regex state.

Resolved:
- `Route::compiled_regex` exposes runtime-only regex state in the core public API.

Notes:
- Timestamp (UTC): 2025-12-30T03:18:52Z
- Limitations: No downstream API consumer audit performed.

### Review 2025-12-29T17:42:57Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: all crates (public API surface scan)

Summary:
- Identified two public APIs that either bypass validation or expose runtime-only state.

Findings:
- [NEW] `ValidatedRuntimeConfig::from_trusted` is public and can bypass semantic validation
  - Description: Public constructor allows callers to wrap unchecked `RuntimeConfig` without validation.
  - Evidence: `crates/pavis-core/src/runtime.rs` (`pub fn from_trusted`).
  - Impact: External crates can bypass canonical validation, undermining boundary guarantees.
  - Recommendation: Restrict to `pub(crate)` or mark as `unsafe` with explicit docs for trusted-only usage.

- [NEW] `Route::compiled_regex` exposes runtime-only regex state in the core public API
  - Description: Core `Route` includes `compiled_regex: Option<regex::Regex>` as a public field.
  - Evidence: `crates/pavis-core/src/runtime/routing.rs`.
  - Impact: Couples core API to runtime details and forces `regex` into core public types.
  - Recommendation: Move compiled regex to runtime-specific wrappers or make it private with accessor hooks.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T17:42:57Z
- Limitations: Downstream API consumers not audited.

### Review 2025-12-29T12:52:30Z

Scope:
- Directories or crates covered: report format alignment only

Summary:
- No new public API findings; report structure aligned to template.

Findings:
- None.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T12:52:30Z
- Limitations: Report-only update; no new API scan performed.

### Review 2025-12-29

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: public API boundary fixes in `pavis` and `pavis-pvs`

Summary:
- Resolved all previously reported public API boundary leaks.
- No new public API concerns identified in this pass.

Findings:
- None.

Resolved:
- [DONE] `pavis-pvs` exposes semantic validation via `load_validated`
  - Resolution summary: Removed public `load_validated` from `pavis-pvs` API surface.
  - Reference: local edit
- [DONE] Runtime exposes internal routing representation
  - Resolution summary: `CompiledVirtualHost` visibility reduced to crate-private.
  - Reference: local edit
- [DONE] Runtime exposes internal load-balancing state
  - Resolution summary: `AlignedCounter` visibility reduced to crate-private.
  - Reference: local edit
- [DONE] Runtime exposes request context type
  - Resolution summary: `RouterContext` visibility reduced and re-export removed.
  - Reference: local edit

Notes:
- Timestamp (UTC): 2025-12-29T12:47:09Z
- Limitations: No downstream consumer audit performed.

### Review 2025-12-29

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: all crates (public API surface scan)

Summary:
- Identified four public API exposures that leak internal details or cross-layer responsibilities.
- Primary boundary concern: `pavis-pvs` exposes semantic validation to callers.
- Runtime crate has multiple public types that appear internal-only.

Findings:
- [NEW] `pavis-pvs` exposes semantic validation via `load_validated`
  - Description: Public API includes semantic validation in the protocol layer.
  - Evidence: `crates/pavis-pvs/src/lib.rs` (`pub use verify::{load, load_validated, verify}`)
  - Impact: Encourages callers to rely on validation in the wrong layer; increases coupling to core semantics.
  - Recommendation: Move validation to codec/relay boundary and keep `pavis-pvs` integrity-only.

- [NEW] Runtime exposes internal routing representation
  - Description: `CompiledVirtualHost` is public though intended for internal matching.
  - Evidence: `crates/pavis/src/router.rs` (`pub struct CompiledVirtualHost`)
  - Impact: External users can depend on internal regex compilation details.
  - Recommendation: Reduce visibility to `pub(crate)` and expose only stable APIs.

- [NEW] Runtime exposes internal load-balancing state
  - Description: `AlignedCounter` is public but only used inside upstream selection.
  - Evidence: `crates/pavis/src/upstream/cluster.rs` (`pub struct AlignedCounter`)
  - Impact: Public API surface includes internal performance implementation details.
  - Recommendation: Limit visibility to module or crate.

- [NEW] Runtime exposes request context type
  - Description: `RouterContext` is public and re-exported but used as proxy internal state.
  - Evidence: `crates/pavis/src/proxy/context.rs` and `crates/pavis/src/proxy.rs`
  - Impact: External coupling to runtime internals.
  - Recommendation: Make `RouterContext` crate-private unless a stable external API is required.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T12:40:26Z
- Limitations: Surface scan only; did not audit downstream consumers.
