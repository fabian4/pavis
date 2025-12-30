# Architecture Compliance Review

## Active Findings (Latest)

- None.

## Historical Reviews

### Review 2025-12-30T03:18:52Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-core`, `crates/pavis`

Summary:
- Removed runtime-only regex storage from the core model and moved compiled state into the runtime router wrapper.

Findings:
- [DONE][LOW] `pavis-core` embeds runtime-only `compiled_regex` in the canonical `Route` model
  - Evidence: `crates/pavis-core/src/runtime/routing.rs` no longer defines `compiled_regex`; runtime compilation now stored in `crates/pavis/src/router.rs`.
  - Resolution: Core model is canonical-only; runtime wrapper stores compiled regex.

Resolved:
- `pavis-core` embeds runtime-only `compiled_regex` in the canonical `Route` model.

Notes:
- Timestamp (UTC): 2025-12-30T03:18:52Z
- Limitations: Focused on routing model boundary only.

### Review 2025-12-30T03:08:16Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-relay`

Summary:
- Confirmed relay `.pvs` validation now delegates to `pavis-pvs`, resolving the prior boundary issue.

Findings:
- [DONE][MEDIUM] `pavis-relay` parses and validates `.pvs` headers outside `pavis-pvs`
  - Evidence: `crates/pavis-relay/src/handlers.rs` uses `pavis_pvs::verify` and `pavis_pvs::inspect`; relay-local parsing module removed.
  - Resolution: Relay now relies on `pavis-pvs` integrity helpers.

Resolved:
- `pavis-relay` parses and validates `.pvs` headers outside `pavis-pvs`.

Notes:
- Timestamp (UTC): 2025-12-30T03:08:16Z
- Limitations: Focused on relay integrity boundary only.

### Review 2025-12-29T17:42:57Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: full workspace

Summary:
- Found two boundary deviations: relay-level PVS parsing bypasses `pavis-pvs`, and core carries runtime-only regex state.

Findings:
- [NEW][MEDIUM] `pavis-relay` parses and validates `.pvs` headers outside `pavis-pvs`
  - Expected: `.pvs` inspection (magic/version/checksum) is owned by `pavis-pvs` only.
  - Evidence: `crates/pavis-relay/src/pvs.rs` implements `parse_header` and `validate` for magic/version/checksum.
  - Deviation: Relay performs integrity parsing directly instead of delegating to `pavis-pvs`.
  - Impact: Boundary drift risks divergent validation logic between relay and `pavis-pvs`.
  - Recommendation: Replace relay-local parsing with `pavis-pvs` helpers (e.g., `read_header`/`verify`) and keep relay to orchestration.

- [NEW][LOW] `pavis-core` embeds runtime-only `compiled_regex` in the canonical `Route` model
  - Expected: Runtime-only fields live in runtime wrappers; core holds canonical semantics only.
  - Evidence: `crates/pavis-core/src/runtime/routing.rs` defines `Route::compiled_regex: Option<regex::Regex>`.
  - Deviation: Core now depends on runtime-only regex compilation state.
  - Impact: Couples core to runtime concerns and exposes a runtime detail in the public API.
  - Recommendation: Move compiled regex storage to runtime wrapper structs, leaving core `Route` purely declarative.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T17:42:57Z
- Limitations: Review focused on code-level boundary adherence; deployment or build-time layering not evaluated.
