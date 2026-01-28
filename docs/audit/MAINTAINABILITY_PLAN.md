# Maintainability Remediation Plan (Excluding `crates/pavis/src/agent.rs`)

_Last updated: 2026-01-28_

This plan sequences the refactors identified in `docs/audit/MAINTAINABILITY_SCAN.md`, skipping `crates/pavis/src/agent.rs` and its dependents (covered by a separate plan). Each workstream lists goals, concrete tasks, dependencies, and validation steps.

Agent note:
- The reload FSM refactor (FSM + driver split) has landed in `crates/pavis/src/agent/*` with expanded unit/integration coverage and new mock-relay request tracing for E2E assertions. This plan remains scoped to non-agent areas.
  - Follow-up findings from agent re-scan (see below) should be tracked separately from the main phases.

### Agent Follow‑Up (Post‑FSM Refactor)
This section documents targeted cleanup items discovered in the agent stack after the FSM/driver split.

1. **Ensure Shutdown always transitions to `Stopped`**
   - **Issue:** `Event::Shutdown` is a no-op in `State::Idle`, so shutdown can leave the FSM non-terminal if it fires before `Start`.
   - **Fix:** Add an explicit `State::Idle + Event::Shutdown => State::Stopped` transition; add a unit test to pin the behavior.
   - **Files:** `crates/pavis/src/agent/fsm.rs`

2. **Backoff duplication / unused `_backoff`**
   - **Issue:** `Backoff` exists but is unused by the FSM; `ConfigAgent` stores `_backoff` only to satisfy API/tests.
   - **Fix options:**
     - **A (preferred):** Remove `_backoff` from `ConfigAgent` and delete `agent::backoff` if no other callers remain; update tests accordingly.
     - **B:** Wire `Backoff` into FSM/driver (single source of truth), replace `backoff_delay()` and constants with `Backoff::next_delay()`.
   - **Files:** `crates/pavis/src/agent/driver.rs`, `crates/pavis/src/agent/backoff.rs`, `crates/pavis/src/agent/fsm.rs`

3. **`write_atomic` is not atomic**
   - **Issue:** `write_atomic()` writes directly to the target path without a temp + rename.
   - **Fix:** Write to a temp file under the same directory, `fsync`, then `rename` to the target; update tests to validate the temp naming convention.
   - **Files:** `crates/pavis/src/agent/lkg.rs`

4. **`Context.local_lkg_path` unused**
   - **Issue:** `local_lkg_path` is stored in FSM context but not referenced in FSM logic or summaries.
   - **Fix:** Either remove the field (and associated constructor wiring) or use it in `StateSummary`/diagnostics where it provides value (e.g., expose in `current_state()` only if used by callers).
   - **Files:** `crates/pavis/src/agent/fsm.rs`, `crates/pavis/src/agent/driver.rs`

## Phase 0 – Prerequisites & Guardrails
1. **Stabilize current behavior**
   - Snapshot baseline by running `make ci-local` and key TLS E2E suites (`70_security_tls.sh`, `71_security_inbound_mtls.sh`, `74_security_mtls_outbound.sh`).
   - Enable `RUST_LOG=pavis=debug` logging in staging to capture metrics and router context traces for comparison post-refactor.
2. **Add regression scaffolding**
   - Create a `tests/proxy/request_planning.rs` module to host new unit tests (reuse existing test exports).
   - Introduce helpers for mocking `RuntimeState` and `RouterContext` to avoid repeatedly wiring Pingora.

## Phase 1 – Bootstrap & Listener Boundaries (Findings 1–2)
**Goal:** Isolate runtime planning and TLS wiring.

Tasks:
1. Create `crates/pavis/src/bootstrap/mod.rs` with:
   - `BootstrapPlan` (holds telemetry handles, state handle, resolver/health services, Pingora conf).
   - Functions `plan_runtime(&ValidatedRuntimeConfig, &Args)`, `build_listeners(&ValidatedRuntimeConfig, &Telemetry)`, `wire_agents(...)`.
2. Move Pingora setup from `main.rs` into BootstrapPlan; expose a small `run(plan)` entry point.
3. Introduce `listener::tls::TlsRuntime` that converts `pavis_core::TlsConfig` into `TlsSettings` (handles client auth, future OCSP/ALPN).
4. Unit tests covering:
   - Plan creation without relay URL.
   - TLS runtime building optional/required client auth with mocked file paths (use temp cert fixtures).

Validation:
- `make ci-local` + targeted TLS E2E suite.
- Manual smoke: start runtime with and without relay URL.

## Phase 2 – Proxy Context Phases & Telemetry (Findings 5,7,8,17,20)
**Goal:** Enforce lifecycle invariants and decouple telemetry.

Tasks:
1. Split `proxy/service/request_planning.rs` into submodules: `id.rs`, `rewrite.rs`, `tls.rs`, `rbac.rs`, `timeouts.rs`. Export a `RequestPlanning` facade for IO layer.
2. Introduce phase-typed structs:
   - `RoutingContext` (pre-match, owns immutable data and `RequestTelemetry`).
   - `RouteMatch` (wraps matched route + RBAC decision).
   - `UpstreamAttempt` (owns pool/circuit permits, rewritten URI/host, retry state).
3. Create `RequestTelemetry` struct (span + helpers) and remove tracing helpers from `RouterContext`.
4. Add `SpiffeId` newtype in `pavis_core`, update RBAC comparisons and `extract_client_identity` to return `Option<SpiffeId>`.
5. Refactor `get_upstream_peer` into `UpstreamPeerBuilder` with methods `apply_tls`, `apply_timeouts`, `attach_metrics`.

Tests:
- Unit tests for context transitions (cannot access permits before `UpstreamAttempt`).
- RBAC tests comparing `SpiffeId` equality/prefix enforcement.
- `UpstreamPeerBuilder` tests using fake clusters (verify TLS + timeout settings).

Validation:
- `make test` + `tests/suites/pavis/70_security_tls.sh`.

## Phase 3 – Endpoint Resolution & Request Planning Hardening (Findings 6,18,19)
**Goal:** Remove per-request DNS and lock down hashing/rewrites.

Tasks:
1. Extend `RuntimeState::from_config` to produce `ResolvedEndpointAddr` entries (resolve DNS via existing resolver or `ToSocketAddrs` once per reload). Store in `upstream::Manager`.
2. Update routing/proxy layers to consume resolved addresses; ensure resolver refresh still works for Strict/Logical discovery.
3. Add unit tests for `reuse_key_hash` (different cert paths, verify modes). Store in new `tests/proxy/request_planning.rs`.
4. Add rewrite tests covering prefix match success, regex skip, unmatched prefix logging. Use `tracing-test` or capture logs.

Validation:
- Run `make ci-local`.
- Manual test: configure DNS endpoint pointing at mock upstream; ensure runtime resolves once per reload (inspect logs/metrics).

## Phase 4 – Router/Runtime Materialization & Cluster Boundaries (Findings 9–16)
**Goal:** Reduce module bandwidth and encode health/pool invariants.

Tasks:
1. Split `crates/pavis/src/upstream/cluster.rs` into:
   - `cluster/state.rs` (ArcSwap state + endpoint selection).
   - `cluster/health.rs` (new `EndpointHealthState` enum, `HealthTracker`).
   - `cluster/pool.rs` (PoolController, QueueDiscipline trait, PoolMetrics helper, PermitBundle drop semantics).
   - `cluster/tls.rs` (client cert & CA bundle handling shared with health monitor).
2. Introduce `MaterializedRuntimeConfig` (router + upstream materials) created from `ValidatedRuntimeConfig`; `RuntimeState` stores this, limiting recomputation.
3. Add `ConfigVersion` newtype (non-zero) stored in `RuntimeState`, update telemetry metrics to use stringified version once, not `Option` checks.

Tests:
- Health tracker transition tests (failures trigger ejection until `eject_duration`).
- Pool queue discipline test verifying metrics increments.
- Materialized runtime smoke test comparing router caches before/after reload.

Validation:
- `make test` (unit focus) + at least one E2E (routing suite) to ensure behavior parity.

## Phase 5 – Health Monitor & TLS Reuse (Findings 10–14)
**Goal:** Separate scheduler/executor, share TLS materialization, simplify metrics endpoint.

Tasks:
1. Build `HealthProbePlan` (interval, timeout, client identity) and `Scheduler` that yields `ProbeJob`s; `Executor` drives probes via `tokio::spawn`.
2. Extract `ClientIdentityMaterializer` used by both `upstream::Manager` and health monitor to avoid duplicated PEM parsing.
3. Refactor metrics worker:
   - `MetricsRegistry` (wraps Prometheus recorder handle).
   - `PrometheusEndpoint` (HTTP listener + transport trait for testability).
4. Extend health monitor tests (async) to ensure disabled checks stop scheduling and intervals are honored.

Validation:
- Run health-related E2E tests (`tests/suites/pavis/50_health_*` if available) or craft targeted integration with mock upstream.
- Manual `curl` against `/metrics` to ensure endpoint still serves data.

## Phase 6 – Documentation & Verification
1. Update docs:
   - `ARCHITECTURE.md`: describe new bootstrap phases, context phase types, cluster module split.
   - `docs/operations/runtime.md`: note that runtime pre-resolves DNS and that metrics endpoint is modular.
2. Add a changelog entry or release note summarizing maintainability improvements.
3. Full `make ci-local` + selected E2E suites (TLS + health + routing) before merge.

## Out-of-Scope
- Any modifications to `crates/pavis/src/agent.rs` or its reload phases—tracked separately.
- External API changes beyond new internal types (`SpiffeId`, `ConfigVersion`).

## Sequencing & Dependencies
1. **Phase 1** must land first (bootstrap reorg affects rest). Feature-flag if necessary.
2. **Phase 2** depends on Phase 1 telemetry handles but can start once bootstrap module exists.
3. **Phase 3** depends on Phase 2 context modules for clean integration.
4. **Phase 4** requires stable runtime materialization from Phase 3.
5. **Phase 5** can run partly in parallel with Phase 4 but should rebase after TLS sharing changes.

## Acceptance Criteria
- All referenced findings outside agent stack addressed or tracked with follow-up issues.
- New unit/integration tests added for each risk area (request planning, context phases, pool behavior, health scheduling).
- Documentation updated to reflect architectural changes.
- `make ci-local` + critical E2E suites pass post-refactor.

---

## Execution Handoff (Detailed, Step-by-Step)
This section is the canonical runbook for future agents. Execute phases in order, and run `make ci-local` after **each** phase. Update doc sections as you go.

### Global Preconditions
- **Do not** touch `RuntimeConfig` structure without explicit user instruction.
- Maintain crate boundaries: runtime must not depend on codecs/serde/rkyv.
- Keep diffs small; prefer incremental commits (agent should not create git commits).
- Update docs listed in Phase 6 **when behavior/structure changes**.

### Phase 1 — Bootstrap & Listener Boundaries (Findings 1–2)
**Goal:** isolate startup ordering and TLS materialization.
**Files to touch:**
- `crates/pavis/src/main.rs`
- `crates/pavis/src/bootstrap/mod.rs` (new)
- `crates/pavis/src/listener/tls.rs` (new or existing module)
- `crates/pavis/src/telemetry/*` (wiring adjustments)
- `ARCHITECTURE.md`, `docs/operations/runtime.md` (if bootstrap behavior changes)

**Steps:**
**Checklist:**
- [ ] BootstrapPlan module created and wired into `main.rs`.
- [ ] Listener TLS materialization centralized in `listener::tls::TlsRuntime`.
- [ ] Unit tests added for bootstrap + TLS materialization.
- [ ] Docs updated if startup wiring changed.
- [ ] `make ci-local` passes.

1. Create `crates/pavis/src/bootstrap/mod.rs` with `BootstrapPlan`:
   - Holds telemetry handles, runtime state handle, services list, pingora server config.
   - API: `BootstrapPlan::build(&ValidatedRuntimeConfig, &Args) -> anyhow::Result<Self>` and `BootstrapPlan::run(self)`.
2. Move Pingora setup and service wiring out of `main.rs` into `BootstrapPlan`.
3. Add `listener::tls::TlsRuntime` to convert `pavis_core::TlsConfig` → `TlsSettings`.
4. Wire listeners in bootstrap using `TlsRuntime` instead of inline TLS logic.
5. Add unit tests:
   - Build plan without relay URL.
   - TLS runtime builds optional/required client auth using temp fixtures.
6. Update docs as needed.
7. Run `make ci-local`. Fix failures immediately.

**Exit criteria:** `main.rs` mostly parses args + calls bootstrap; TLS config is centralized.

### Phase 2 — Proxy Context Phases & Telemetry (Findings 5,7,8,17,20)
**Goal:** eliminate Option-soup context and decouple telemetry.
**Files to touch:**
- `crates/pavis/src/proxy/context.rs`
- `crates/pavis/src/proxy/service/request_planning.rs` (split)
- `crates/pavis/src/telemetry/*`
- `crates/pavis-core/src/*` (newtypes)
- Tests: `tests/proxy/*`, `crates/pavis/tests/*`

**Steps:**
**Checklist:**
- [ ] `request_planning` split into focused modules.
- [ ] Phase-typed contexts introduced and wired.
- [ ] `RequestTelemetry` replaces router-context tracing helpers.
- [ ] `SpiffeId` newtype added and RBAC updated.
- [ ] Unit tests added/updated for context phases and RBAC.
- [ ] Docs updated if telemetry/context model changed.
- [ ] `make ci-local` passes.

1. Split `request_planning.rs` into modules: `id.rs`, `rewrite.rs`, `tls.rs`, `rbac.rs`, `timeouts.rs`.
2. Create phase-typed structs:
   - `RoutingContext`, `RouteMatch`, `UpstreamAttempt`.
3. Add `RequestTelemetry` and move tracing helpers out of `RouterContext`.
4. Add `SpiffeId` newtype in `pavis_core`; update RBAC comparisons.
5. Update tests:
   - Context transitions (phase invariants).
   - RBAC tests for `SpiffeId`.
6. Update docs if telemetry surface changes.
7. Run `make ci-local`.

**Exit criteria:** context invariants enforced by types; routing/policy concerns separated.

### Phase 3 — Endpoint Resolution & Request Planning (Findings 6,18,19)
**Goal:** pre-resolve DNS and harden hashing/rewrites.
**Files to touch:**
- `crates/pavis/src/state.rs`
- `crates/pavis/src/upstream/*`
- `crates/pavis/src/proxy/service/request_planning/*`
- Tests in `tests/proxy/request_planning.rs`

**Steps:**
**Checklist:**
- [ ] DNS resolution moved to reload/materialization step.
- [ ] Proxy layers consume resolved endpoints only.
- [ ] `reuse_key_hash` and rewrite edge tests added.
- [ ] Docs updated if DNS behavior changes.
- [x] `make ci-local` passes.

1. Extend `RuntimeState::from_config` to resolve DNS once per reload.
2. Store resolved endpoints in `upstream::Manager` (respect Strict/Logical).
3. Update routing/proxy layers to consume resolved addresses.
4. Add tests for `reuse_key_hash` and rewrite edge cases.
5. Run `make ci-local`.

**Exit criteria:** no per-request DNS; tests cover hash/rewrite invariants.

### Phase 4 — Router/Runtime Materialization & Cluster Boundaries (Findings 9–16)
**Goal:** split cluster module and add materialized runtime barrier.
**Files to touch:**
- `crates/pavis/src/upstream/cluster.rs` → split into submodules.
- `crates/pavis/src/state.rs`
- `crates/pavis/src/router.rs`
- New type `MaterializedRuntimeConfig`.
- Add `ConfigVersion` newtype.

**Steps:**
**Checklist:**
- [x] `cluster` split into state/health/pool/tls submodules.
- [x] `MaterializedRuntimeConfig` introduced and stored in `RuntimeState`.
- [x] `ConfigVersion` newtype plumbed through metrics/state.
- [x] Health/pool tests refreshed (existing suite now runs through the split modules).
- [x] Docs updated for materialization boundary.
- [x] `make ci-local` passes.

1. Split `cluster.rs` into `cluster/state.rs`, `cluster/health.rs`, `cluster/pool.rs`, `cluster/tls.rs`.
2. Introduce `MaterializedRuntimeConfig` and store in `RuntimeState`.
3. Add `ConfigVersion` newtype; update metrics plumbing to avoid `Option` churn.
4. Add tests for health tracker transitions and pool queue metrics.
5. Run `make ci-local`.

**Exit criteria:** cluster responsibilities separated; runtime materialization barrier exists.

### Phase 5 — Health Monitor & TLS Reuse (Findings 10–14)
**Goal:** separate scheduling/execution and dedupe TLS parsing.
**Files to touch:**
- `crates/pavis/src/upstream/health.rs`
- `crates/pavis/src/telemetry/metrics.rs`
- Shared TLS materializer module.

**Steps:**
**Checklist:**
- [x] Health scheduler/executor split (plan + jobs).
- [x] Shared client identity materializer extracted.
- [x] Metrics registry + endpoint split.
- [x] Health monitor async tests updated.
- [x] Docs updated if metrics/health internals change.


1. Create `HealthProbePlan` + `Scheduler` → yields `ProbeJob`s.
2. Build `Executor` to run probes (`tokio::spawn`).
3. Extract shared `ClientIdentityMaterializer`.
4. Refactor metrics worker: `MetricsRegistry` + `PrometheusEndpoint`.
5. Extend health monitor tests to cover scheduling / disabled behavior.
6. Run `make ci-local`.

**Exit criteria:** health probe scheduling is isolated; TLS parsing shared; metrics endpoint modular.

### Phase 6 — Docs & Verification
After each phase and at the end:
**Checklist:**
- [x] `ARCHITECTURE.md` updated for structural changes.
- [x] `docs/operations/runtime.md` updated for runtime behavior changes.
- [ ] `docs/roadmap/roadmap.md` + `docs/roadmap/features.md` refreshed as milestones complete.
- [ ] `make ci-local` passes (per phase).

1. Update `ARCHITECTURE.md` and `docs/operations/runtime.md` where behavior changes.
2. Refresh `docs/roadmap/roadmap.md` + `docs/roadmap/features.md` if milestones are completed.
3. Ensure `make ci-local` is green.
