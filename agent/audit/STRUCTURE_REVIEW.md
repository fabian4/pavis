# Rust Code Structure & File Size Review

## Active Findings (Latest)

- None.

## Historical Reviews

### Review 2025-12-29T18:09:48Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-relay`, `crates/pavis`, `crates/pavctl`

Summary:
- Split relay config handling into focused modules, moved proxy service tests out of the runtime file, and separated pavctl formatting/parsing utilities.

Findings:
- [DONE] `crates/pavis-relay/src/config.rs` combines config schema, parsing, env expansion, and tests in a single 673-line file
  - Evidence: `crates/pavis-relay/src/config/` now contains `types.rs`, `load.rs`, `env.rs`, and `tests.rs`.
  - Resolution: `crates/pavis-relay/src/config.rs` now acts as a thin module re-exporter.

- [DONE] `crates/pavis/src/proxy/service.rs` mixes core proxy logic with extensive test helpers and cases
  - Evidence: Tests moved into `crates/pavis/src/proxy/service/service_tests.rs`.
  - Resolution: `crates/pavis/src/proxy/service.rs` now contains only runtime logic plus a test module declaration.

- [DONE] `crates/pavctl/src/lib.rs` bundles parsing, formatting, and stats output without module separation
  - Evidence: `crates/pavctl/src/format.rs` and `crates/pavctl/src/parse.rs` now host the logic.
  - Resolution: `crates/pavctl/src/lib.rs` now re-exports the focused modules.

Resolved:
- `crates/pavis-relay/src/config.rs` combines config schema, parsing, env expansion, and tests in a single 673-line file.
- `crates/pavis/src/proxy/service.rs` mixes core proxy logic with extensive test helpers and cases.
- `crates/pavctl/src/lib.rs` bundles parsing, formatting, and stats output without module separation.

Notes:
- Timestamp (UTC): 2025-12-29T18:09:48Z
- Limitations: Structural refactor only; behavior assumed unchanged.

### Review 2025-12-29T17:42:57Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: full workspace

Summary:
- Identified three large or multi-responsibility modules that would benefit from feature- or responsibility-based splits.

Findings:
- [NEW] `crates/pavis-relay/src/config.rs` combines config schema, parsing, env expansion, and tests in a single 673-line file
  - Evidence: `crates/pavis-relay/src/config.rs` defines all config structs, parsing helpers, env expansion, and tests.
  - Impact: Harder to navigate and reason about changes; increases merge conflicts.
  - Recommendation: Split into `config/types.rs` (structs), `config/load.rs` (decode/normalize), `config/env.rs` (env expansion), with tests in `config/tests.rs`.

- [NEW] `crates/pavis/src/proxy/service.rs` mixes core proxy logic with extensive test helpers and cases
  - Evidence: `crates/pavis/src/proxy/service.rs` contains Proxy implementation plus multiple `#[cfg(test)]` modules and helper functions.
  - Impact: Production logic and tests are interleaved, slowing navigation and review.
  - Recommendation: Move test helpers/tests into `crates/pavis/src/proxy/tests.rs` or integration tests, keeping `service.rs` focused on runtime behavior.

- [NEW] `crates/pavctl/src/lib.rs` bundles parsing, formatting, and stats output without module separation
  - Evidence: `crates/pavctl/src/lib.rs` includes runtime parsing, header formatting, config formatting, and stats formatting in one file.
  - Impact: Cross-cutting concerns are co-located, making future command additions harder to isolate.
  - Recommendation: Extract `format.rs` (header/config/stats), `parse.rs` (codec materialization), and keep `lib.rs` as a thin re-export layer.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T17:42:57Z
- Limitations: Structural review only; no refactors performed.

### Review 2025-12-29T14:12:48Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-relay`

Summary:
- Verified `pavis-relay` is now split into feature-focused modules with tests moved out of `src/`.
- No new structural issues observed.

Findings:
- [DONE] `crates/pavis-relay/src/lib.rs` combines routing setup, HTTP handlers, state/cache, and tests in one module
  - Evidence: Previously inlined in `crates/pavis-relay/src/lib.rs`; now split into `crates/pavis-relay/src/state.rs`, `crates/pavis-relay/src/handlers.rs`, `crates/pavis-relay/src/routes.rs`, with tests in `crates/pavis-relay/tests/relay_http.rs`.
  - Resolution: Modularized by responsibility and moved tests into integration tests.

- [DONE] `crates/pavis-relay/src/lib.rs` is a single-file root with multiple unrelated features
  - Evidence: Long-poll, publish, artifacts, metrics/health now separated across modules.
  - Resolution: Feature buckets isolated into distinct modules.

Resolved:
- `crates/pavis-relay/src/lib.rs` combines routing setup, HTTP handlers, state/cache, and tests in one module
- `crates/pavis-relay/src/lib.rs` is a single-file root with multiple unrelated features

Notes:
- Timestamp (UTC): 2025-12-29T14:12:48Z
- Limitations: Focused on `pavis-relay` per user request; other crates not reviewed in this pass.

### Review 2025-12-29T14:07:12Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-relay`

Summary:
- Confirmed `pavis-relay` remains a single-file module with multiple responsibilities; no structural changes since last review.
- No refactors performed; recommendations only.

Findings:
- [EXISTING] `crates/pavis-relay/src/lib.rs` combines routing setup, HTTP handlers, state/cache, and tests in one module
  - Evidence: `RelayState`, `RelaySnapshot`, `router`, `serve`, handlers (`get_config`, `post_publish`, `get_metrics`), and tests co-located in `crates/pavis-relay/src/lib.rs`.
  - Impact: Blurs responsibility boundaries and makes it harder to navigate or evolve individual concerns.
  - Recommendation: Split into `state.rs` (state + storage), `handlers.rs` (HTTP handlers), `server.rs` or `routes.rs` (router assembly), and keep tests in `crates/pavis-relay/tests/` or `src/lib.rs` test module per concern.

- [EXISTING] `crates/pavis-relay/src/lib.rs` is a single-file root with multiple unrelated features
  - Evidence: Long-poll logic, publish/version control, metrics output, and artifact history are all implemented in one file.
  - Impact: Feature-driven navigation is slow and increases the risk of accidental coupling.
  - Recommendation: Extract feature buckets (config fetch, publish, artifacts, metrics/health) into separate modules.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T14:07:12Z
- Limitations: Focused on `pavis-relay` per user request; other crates not reviewed in this pass.

### Review 2025-12-29T13:43:40Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: `crates/pavis-relay`

Summary:
- Identified structural concentration in `pavis-relay` where routing, state, handlers, and tests live in a single module.
- No refactors performed; recommendations only.

Findings:
- [NEW] `crates/pavis-relay/src/lib.rs` combines routing setup, HTTP handlers, state/cache, and tests in one module
  - Evidence: `RelayState`, `RelaySnapshot`, `router`, `serve`, handlers (`get_config`, `post_publish`, `get_metrics`), and tests co-located in `crates/pavis-relay/src/lib.rs`.
  - Impact: Blurs responsibility boundaries and makes it harder to navigate or evolve individual concerns.
  - Recommendation: Split into `state.rs` (state + storage), `handlers.rs` (HTTP handlers), `server.rs` or `routes.rs` (router assembly), and keep tests in `crates/pavis-relay/tests/` or `src/lib.rs` test module per concern.

- [NEW] `crates/pavis-relay/src/lib.rs` is a single-file root with multiple unrelated features
  - Evidence: Long-poll logic, publish/version control, metrics output, and artifact history are all implemented in one file.
  - Impact: Feature-driven navigation is slow and increases the risk of accidental coupling.
  - Recommendation: Extract feature buckets (config fetch, publish, artifacts, metrics/health) into separate modules.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T13:43:40Z
- Limitations: Focused on `pavis-relay` per user request; other crates not reviewed in this pass.
