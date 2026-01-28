# Maintainability Remediation Plan (Excluding `crates/pavis/src/agent.rs`)

_Last updated: 2026-01-28_

This plan sequences the refactors identified in `docs/audit/MAINTAINABILITY_SCAN.md`, skipping `crates/pavis/src/agent.rs` and its dependents (covered by a separate plan). Each workstream lists goals, concrete tasks, dependencies, and validation steps.

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
