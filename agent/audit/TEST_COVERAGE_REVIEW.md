# Test Coverage & Quality Review

## Active Findings (Latest)

- None.

## Historical Reviews

### Review 2025-12-29T14:05:12Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: full workspace (unit, integration, and e2e tests)

Summary:
- Added relay long-poll update delivery coverage with header and body assertions.
- No remaining active findings from prior review.

Findings:
- [DONE] `pavis-relay` does not test long-poll update delivery or response headers
  - Evidence: `crates/pavis-relay/tests/relay_http.rs` now asserts `content-type`, `x-pavis-version`, and updated body after a publish.
  - Resolution: Added `config_long_poll_returns_update_with_headers` test.

Resolved:
- `pavis-relay` does not test long-poll update delivery or response headers

Notes:
- Timestamp (UTC): 2025-12-29T14:05:12Z
- Limitations: None noted.

### Review 2025-12-29T14:03:11Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: full workspace (unit, integration, and e2e tests)

Summary:
- Added unit tests for `pavis-ingest-api` and `pavis-codec-api` constructor/flow coverage.
- Remaining gap is the relay long-poll update delivery and header assertions.

Findings:
- [DONE] `pavis-ingest-api` has no tests for core constructors or error handling
  - Evidence: Tests added in `crates/pavis-ingest-api/src/lib.rs` for `Artifact`, `SourceInfo`, and `IngestError`.
  - Resolution: Added unit coverage for defaults and conversions.

- [DONE] `pavis-codec-api` public API lacks tests for `Codec::materialize` flow
  - Evidence: Tests added in `crates/pavis-codec-api/src/lib.rs` using a mock codec to cover check/compile/validation paths.
  - Resolution: Added unit tests for `materialize` success and error propagation.

- [EXISTING] `pavis-relay` does not test long-poll update delivery or response headers
  - Evidence: `get_config` sets `content-type` and `x-pavis-version`, and should return updated bytes after a publish (`crates/pavis-relay/src/handlers.rs`); tests cover 304 timeout but not update delivery in `crates/pavis-relay/tests/relay_http.rs`.
  - Impact: Header regressions and update delivery behavior could slip through.
  - Recommendation: Add a test that publishes a new version while a long-poll is waiting and assert headers plus body content.

Resolved:
- `pavis-ingest-api` has no tests for core constructors or error handling
- `pavis-codec-api` public API lacks tests for `Codec::materialize` flow

Notes:
- Timestamp (UTC): 2025-12-29T14:03:11Z
- Limitations: Relay update-delivery behavior remains untested.

### Review 2025-12-29T13:58:47Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: full workspace (unit, integration, and e2e tests)

Summary:
- Verified relay gaps from the prior review were addressed with new tests.
- Found remaining coverage gaps in small API crates and relay update/header assertions.

Findings:
- [DONE] `pavis-relay` long-poll `NOT_MODIFIED` path is untested
  - Evidence: `crates/pavis-relay/tests/relay_http.rs` now asserts `StatusCode::NOT_MODIFIED` for a matching version with short `wait_ms`.
  - Resolution: Added `config_long_poll_returns_not_modified` test.

- [DONE] `pavis-relay` publish error paths lack coverage
  - Evidence: `crates/pavis-relay/tests/relay_http.rs` covers missing header (`BAD_REQUEST`) and monotonicity rejection (`CONFLICT`).
  - Resolution: Added `publish_requires_version_header` and `publish_rejects_non_increasing_version` tests.

- [DONE] `pavis-relay` artifact and metrics bodies are not asserted
  - Evidence: `crates/pavis-relay/tests/relay_http.rs` now asserts artifact 404 body and metrics content.
  - Resolution: Added `artifact_and_metrics_bodies_are_stable` test.

- [NEW] `pavis-ingest-api` has no tests for core constructors or error handling
  - Evidence: No `#[cfg(test)]` modules or tests under `crates/pavis-ingest-api/`; `crates/pavis-ingest-api/src/lib.rs` defines `Artifact`, `SourceInfo`, and `IngestError` without test coverage.
  - Impact: API contract drift (e.g., defaulting behavior for `Artifact::new`, `SourceInfo::unknown`) would be undetected.
  - Recommendation: Add unit tests for constructors, default fields, and `IngestError` conversions.

- [NEW] `pavis-codec-api` public API lacks tests for `Codec::materialize` flow
  - Evidence: No tests under `crates/pavis-codec-api/`; `crates/pavis-codec-api/src/lib.rs` includes `Codec::materialize` with error propagation and validation behavior.
  - Impact: Potential regressions in codec error mapping and validation behavior without coverage.
  - Recommendation: Add a small mock codec test that exercises `check`, `compile`, and `materialize` error paths.

- [NEW] `pavis-relay` does not test long-poll update delivery or response headers
  - Evidence: `get_config` sets `content-type` and `x-pavis-version`, and should return updated bytes after a publish (`crates/pavis-relay/src/handlers.rs`); tests only cover status codes and a 304 long-poll timeout in `crates/pavis-relay/tests/relay_http.rs`.
  - Impact: Header regressions and update delivery behavior could slip through.
  - Recommendation: Add a test that publishes a new version while a long-poll is waiting and assert headers plus body content.

Resolved:
- `pavis-relay` long-poll `NOT_MODIFIED` path is untested
- `pavis-relay` publish error paths lack coverage
- `pavis-relay` artifact and metrics bodies are not asserted

Notes:
- Timestamp (UTC): 2025-12-29T13:58:47Z
- Limitations: Coverage review emphasized visible unit/integration tests; deeper behavioral coverage may still require targeted e2e additions.

### Review 2025-12-29T13:51:13Z

Scope:
- Commit / branch / tag reviewed: local workspace (uncommitted)
- Directories or crates covered: workspace tests and unit tests (with emphasis on `pavis-relay`)

Summary:
- Broad unit and e2e coverage exists across `pavis`, `pavis-core`, `pavis-pvs`, `pavctl`, and `pavis-e2e`.
- Identified gaps in relay HTTP negative-path and long-poll behavior coverage.

Findings:
- [NEW] `pavis-relay` long-poll `NOT_MODIFIED` path is untested
  - Evidence: `get_config` returns `StatusCode::NOT_MODIFIED` when the client version matches and no update arrives (`crates/pavis-relay/src/handlers.rs`); tests only assert `StatusCode::OK` in `crates/pavis-relay/tests/relay_http.rs`.
  - Impact: Regression risk for the long-poll behavior (timeouts, version equality) without test coverage.
  - Recommendation: Add a test that sets `x-pavis-version` to the current version, uses a short `wait_ms`, and asserts a 304 response.

- [NEW] `pavis-relay` publish error paths lack coverage
  - Evidence: `post_publish` returns `BAD_REQUEST` for missing `x-pavis-version` and `CONFLICT` on monotonicity violation via `RelayState::publish` (`crates/pavis-relay/src/handlers.rs`, `crates/pavis-relay/src/state.rs`); tests only cover the happy path in `crates/pavis-relay/tests/relay_http.rs`.
  - Impact: Header parsing and version enforcement regressions could slip through.
  - Recommendation: Add tests for missing/invalid `x-pavis-version` and for rejecting a non-increasing version.

- [NEW] `pavis-relay` artifact and metrics bodies are not asserted
  - Evidence: `get_artifact` returns 404 for unknown versions and `get_metrics` emits Prometheus text (`crates/pavis-relay/src/handlers.rs`); tests only assert status codes and do not validate bodies (`crates/pavis-relay/tests/relay_http.rs`).
  - Impact: Response formats may drift without detection.
  - Recommendation: Add tests for 404 responses on unknown versions and basic body assertions for metrics output.

Resolved:
- None.

Notes:
- Timestamp (UTC): 2025-12-29T13:51:13Z
- Limitations: Focused deeper on `pavis-relay` gaps after the recent refactor; other crates received a high-level pass.
