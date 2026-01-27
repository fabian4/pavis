# Runtime Reload FSM Refactor Execution Plan

## 1) Scope and Deliverables

This refactor will introduce a pure state-machine module for runtime config fetch/reload and a separate driver/executor layer for I/O. Deliverables include:
- New FSM module with pure `tick(event) -> Vec<Effect>` and `current_state() -> StateSummary` APIs.
- Driver/executor that translates Effects into I/O actions (HTTP fetch, verify, apply, timers, LKG persistence).
- Integration changes in the runtime agent to route events through the FSM and execute effects.
- Unit tests for FSM transitions and effect emission.
- Integration tests using a mocked relay server.
- E2E tests using existing harness to validate real runtime/relay behavior.
- Documentation updates reflecting the FSM model, events, and effects.

## 2) Target Architecture

The final architecture splits runtime reload into two layers:
- **FSM module (pure logic)**: deterministic state transitions; no I/O; exposes only `tick` and `current_state`.
- **Driver/Executor**: executes effects (HTTP requests, verification, apply, timers, LKG persistence), then feeds resulting events into the FSM.

Proposed Rust paths:
- FSM module:
  - `crates/pavis/src/agent/fsm.rs` (State, Event, Effect, Context, constants, `tick`, `current_state`)
- Driver/Executor:
  - `crates/pavis/src/agent/driver.rs`
- Integration points:
  - `crates/pavis/src/agent/worker/agent.rs` (replace direct logic with driver + FSM)
  - `crates/pavis/src/agent/lkg.rs` (ensure LKG operations remain reusable by driver)
  - `crates/pavis/src/agent/mod.rs` (public wiring)

## 3) Step-by-Step Implementation Plan

Step 1: Introduce FSM types and constants without integration. Create `crates/pavis/src/agent/fsm.rs` and define State, Event, Effect, Context (last_applied_etag, last_rejected_etag, last_rejected_until, backoff_attempt, local_lkg_path), constants `WAIT_MS = 30000` and `REJECT_TTL = 10 minutes`, and `StateSummary` for `current_state()`. Run `cargo test -p pavis` to keep the build green.

Step 2: Implement `tick(event)` for all states with full transition table in `crates/pavis/src/agent/fsm.rs`. Keep it pure: no I/O, only returns effects and updated state/context. Add explicit dedup decision after VerifyOk and rejected-etag skip before Verify on Response(NewArtifact). Emit only `ScheduleTimer(duration)` for backoff. Run `cargo test -p pavis`.

Step 3: Add FSM unit tests in `crates/pavis/src/agent/fsm.rs` (module tests). Cover every state + every response class, TTL expiry, dedup skip, rejected-etag skip, backoff scheduling, and NeedResync reset behavior. Run `cargo test -p pavis`.

Step 4: Implement driver/executor skeleton in `crates/pavis/src/agent/driver.rs`. The driver owns the FSM, runs an event loop, executes effects, and feeds resulting events back into the FSM. Ensure no I/O happens inside `tick`. Run `cargo test -p pavis`.

Step 5: Driver startup loads local LKG and applies it directly (Option A). In the driver startup path (before entering the fetch loop): load local LKG from disk, verify, apply, persist if needed, and seed FSM context with `last_applied_etag`. The FSM never performs LKG I/O. Run `cargo test -p pavis`.

Step 6: Map Fetch effects to HTTP requests. In driver, implement `FetchConditional` and `FetchUnconditional` using existing HTTP client code in `agent/worker/agent.rs`. Ensure single in-flight request by executing only when FSM is in Fetching. Translate HTTP/network outcomes to Response(NewArtifact|NoUpdate|TransientUnavailable|NeedResync). Run `cargo test -p pavis`.

Step 7: Map Verify and Apply effects. Implement Verify via existing `.pvs` verification path (checksum vs ETag, format validation, schema/version compatibility). Implement Apply via existing apply/update path and state update hooks. Emit VerifyOk/VerifyFail and ApplyOk/ApplyFail events accordingly. Run `cargo test -p pavis`.

Step 8: Implement timer scheduling. Driver converts `ScheduleTimer(duration)` to a timer; on expiry, emit TimerFired. FSM state does not track deadlines; deadlines are owned by the driver (informational only for logs/metrics if desired). Run `cargo test -p pavis`.

Step 9: Integrate driver into runtime agent. Replace `ConfigAgent::poll_once` loop with driver loop. Keep public API stable for `ConfigAgent::new`, `worker`, and `on_update`. Ensure `current_state()` is exposed for diagnostics. Run `make ci-local`.

Step 10: Remove/redirect legacy logic. Remove now-unused code paths in `agent/worker/agent.rs` that perform direct fetch/verify/apply. Ensure all behavior flows through FSM + driver. Run `make ci-local`.

## 4) Documentation Updates

Update the following docs to align with normative specs (update if exists, else create):
- `docs/specs/runtime-config-fsm.md`: add any missing implementation notes (event/effect mapping, driver separation).
- `docs/operations/runtime.md`: document the new FSM-driven agent behavior, long-poll schedule, and resync semantics.
- `docs/operations/metrics.md`: add runtime FSM metrics and clarify NoUpdate vs backoff.
- `ARCHITECTURE.md`: add a small section describing the FSM/driver split and frozen data plane constraints.

Content updates must include:
- FSM state diagram and transition summary.
- Event/effect mapping and resync/backoff/no-update behaviors.
- Explicit note that the driver executes effects and FSM remains pure.

## 5) Test Plan (Concrete)

### 5.1 Unit Tests
Create tests under `crates/pavis/src/agent/fsm.rs` with a small fixture builder for Context and State. Required test cases:
- Idle + Start triggers no I/O in tick (driver handles LKG load), emits FetchUnconditional(WAIT_MS).
- Fetching + Response(NoUpdate) → Idle + FetchConditional when last_applied_etag present.
- Fetching + Response(NoUpdate) → Idle + FetchUnconditional when last_applied_etag missing.
- Fetching + Response(NeedResync) clears conditional state, resets backoff, emits FetchUnconditional.
- Fetching + Response(TransientUnavailable) → BackoffSleeping with ScheduleTimer(backoff_delay).
- BackoffSleeping + TimerFired → Idle + FetchConditional/FetchUnconditional.
- Verifying + VerifyOk with etag == last_applied_etag → Idle + FetchConditional (dedup skip).
- Verifying + VerifyOk with etag != last_applied_etag → Applying + Apply effect.
- Verifying + VerifyFail sets rejected-etag + TTL and emits Fetch.
- Fetching + Response(NewArtifact) with etag == rejected_etag (TTL valid) skips Verify/Apply and returns to fetch.
- Rejected-etag TTL expiry clears skip.
- Applying + ApplyOk writes LKG, sets last_applied_etag, emits Fetch.
- Applying + ApplyFail sets rejected-etag and emits Fetch.

### 5.2 Integration Tests
Use a mocked relay server in `crates/pavis/tests/` (extend `pavis-testkit` mock relay) with real HTTP client. Record method, path, query string, and headers for each request. Assertions:
- `wait_ms=30000` is present in the query string for long-polls.
- Conditional requests include `If-None-Match: <last_applied_etag>`.
- Unconditional requests omit `If-None-Match`.
- Single in-flight request: no overlapping active requests in the mock server.
- 204/304 do not trigger backoff (next fetch is immediate long-poll).
- 410 triggers unconditional fetch and resets state.
- 5xx triggers backoff scheduling with cap and jitter bounds.

Proposed test file: `crates/pavis/tests/config_agent_fsm_integration.rs`.

### 5.3 E2E Tests
Prefer integrated suite with the real relay. Use mock relay only for behaviors the real relay cannot produce (NeedResync 410 and rejected-etag skip). Integrated suite always uses real `pavis-relay` (binary or Docker). Pavis suite uses scripted mock relay only as the relay peer; the SUT remains the real runtime container even in Docker mode.

Move/adjust into integrated suite (real relay):
- `tests/suites/integrated/30_lkg_artifact.sh`: extend to pre-seed a local LKG on disk before runtime start, then publish a newer relay artifact; assert immediate service from local LKG and eventual convergence to relay version. Use metrics `pavis_runtime_config_version` (or equivalent) to confirm version change.
- `tests/suites/integrated/20_reload_stable.sh`: add a branch to assert repeated identical publishes do not trigger reload (stable version/ETag in metrics).
- `tests/suites/integrated/60_resilience_restart.sh`: add a relay restart window to simulate transient unavailability (connection errors); assert backoff behavior (increasing retry intervals up to cap) and recovery within bounded MTTR after relay resumes.

Keep mock relay for cases the real relay cannot emit:
- `tests/suites/pavis/84_resync_410_forces_unconditional.sh`:
  - Mock relay returns 410 once on `/v1/config`, then 200 on the next unconditional fetch.
  - Assert conditional state cleared (If-None-Match absent on next request), backoff reset, and successful apply.
- `tests/suites/pavis/85_rejected_etag_skip.sh`:
  - Serve a corrupt artifact so runtime rejects it, then serve the exact same corrupt artifact again; assert runtime skips verify/apply and immediately resumes long-poll (no duplicate validation failure counters).

Injection mechanisms:
- Local LKG seeding (integrated): write `config.pvs` into runtime local LKG path before start; ensure agent loads it.
- Transient unavailability (integrated): stop relay process for N seconds, then restart; observe connection errors and backoff.
- 410 and repeated invalid artifacts (mock relay): extend `pavis-testkit` mock relay routes (`crates/pavis-testkit/src/relay/routes`) to support scripted responses per request (sequence: 410 → 200; corrupt payload repeated).

Each new/adjusted case must include explicit assertions for:
- single in-flight fetch (no overlapping requests)
- wait_ms=30000 on long-poll
- 204/304 do not backoff
- 410 resets conditional state and triggers unconditional fetch (mock relay)
- 5xx or connection errors trigger bounded backoff (base=250ms, cap=5000ms, jitter tolerated)

## 6) Rollout and Risk Controls

- Ship in small PRs: (1) FSM types + tests, (2) driver + effect wiring, (3) agent integration, (4) cleanup + docs.
- Preserve behavior by comparing metrics before/after: fetch counts by class, apply/verify counts, backoff attempts.
- CI watchpoints: new FSM unit tests, integration tests, and existing relay/runtime tests must pass.

## 7) Definition of Done

- FSM module implemented with pure `tick(event) -> Vec<Effect>` and `current_state() -> StateSummary`.
- Driver executes effects with no I/O in FSM.
- Single in-flight fetch enforced.
- Dedup/verification rules fully implemented per spec.
- Resync and backoff behaviors match normative FSM spec.
- Local LKG behavior matches spec.
- Documentation updated as listed.
- Unit, integration, and E2E tests added and passing.
- `make ci-local` passes.
