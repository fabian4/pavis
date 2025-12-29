# Rust Code Structure & File Size Review

## Active Findings (Latest)

- None.

## Historical Reviews

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
