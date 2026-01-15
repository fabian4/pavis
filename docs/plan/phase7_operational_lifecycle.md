# Phase 7 Plan: Operational Lifecycle

**Status**: Draft  
**Owner**: Codex  
**Scope**: Phase 7 items in ROADMAP.md (Graceful Shutdown, Admin API)  
**Non-Goals**: New crates, runtime policy expansion beyond Frozen Data Plane, data-plane feature changes, policy expressiveness increases

## 1. Objectives (from ROADMAP.md)
- **Graceful Shutdown**: Connection draining sequences.
- **Admin API**: Read-only runtime inspection endpoints.
 

## 2. Constraints & Architecture Guardrails
- **Frozen Data Plane**: No runtime policy inference or dynamic feature logic.
- **Layering**: `pavis-core` defines config types; codecs materialize defaults; runtime executes validated config only.
- **Zero-Option policy** in `pavis-core` and `pavis` runtime for policy toggles (explicit enums).
- **No new crates**; keep changes in existing modules.
- **Phase boundary**: Phase 7 introduces no new traffic behavior, no L7 feature expansion, and no policy expressiveness increase.

## 3. Decision Gates (Requires Explicit Approval)
1) **RuntimeConfig changes**: any new fields or enums.
2) **Admin API surface**: endpoint names, auth model, and data exposed.
3) **Shutdown semantics**: drain vs immediate stop for each listener.
4) **Admin API data exposure audit**: explicit list of exposed fields; confirm no secrets or raw config bytes leak.

## 4. Proposed Work Plan (Pending Approval of Gates)

### 4.1 Graceful Shutdown
- **Core config**:
  - Add `ShutdownPolicy` (Disabled, Enabled { drain_timeout }).
  - Attach to listener-level config for drain parameters only (shutdown is process-global).
- **Runtime behavior**:
  - Trigger sources: SIGTERM/SIGINT only (no Admin API shutdown trigger in Phase 7).
  - On shutdown signal: stop accepting new connections; drain in-flight requests until `drain_timeout`.
  - After drain timeout: close remaining connections, then exit.
  - Fail-close semantics: reject new connections immediately; drain in-flight only.
  - Define connection classes: in-flight requests, keep-alive idle, long-lived streams (future support) and document drain behavior.
- **Tests**:
  - Unit tests for policy validation and conversion.
  - Integration test: open long-lived connection, trigger shutdown, verify drain then close.

### 4.2 Admin API
**Endpoints (fixed, read-only)**
- `/admin/health`
- `/admin/stats` (counters, connection totals, reload counts)
**Scope**
- Serve on admin listener only.
- Informational and best-effort only; not a stable automation contract in Phase 7.
- Explicitly forbid: config mutation, reload triggers, policy toggles, runtime state mutation.
**Security**
- Prefer bind-address restriction only (127.0.0.1 / unix socket).
- Avoid header-based auth unless explicitly required and documented.
- **Tests**:
  - Unit tests for handler responses.
  - E2E: verify admin endpoints accessible on admin bind only.

## 5. Risks & Mitigations
- **Config churn**: minimize new fields and keep defaults explicit in codecs.
- **Operational exposure**: Admin API must be clearly bounded and secured by binding or auth header.
- **Shutdown race conditions**: ensure drain ordering and idempotent shutdown.

## 6. Documentation Deliverables
- Update `ROADMAP.md` Phase 7 status when items land.
- Update `docs/FEATURES.md` matrix for Graceful Shutdown and Admin API.
- Add `docs/operations.md` (or `docs/admin.md`) covering shutdown lifecycle, admin API contract, tuning scope/exclusions.

## 7. Tests
### Unit Tests
- Config validation for shutdown fields.
- Admin handler response shape checks.

### Integration Tests
- Graceful shutdown drains live connections.
- Admin API only on admin bind.

### E2E Tests
- Full pipeline: publish config, observe admin endpoints reflect version and stats.
- Shutdown scenario with in-flight requests.
- Reload interaction tests:
  - Shutdown during reload.
  - Reload during active connections.
  - Admin API queried during reload.
- Invariant checks:
  - Admin API responses stable across reloads.
  - Admin API does not affect routing/latency behavior.

## 8. Deliverables
- Core API + validation updates for shutdown/admin.
- Codec defaults materialized explicitly.
- Runtime implementation for graceful shutdown and admin endpoints.
- Tests and docs.

## 9. Verification Checklist
- [ ] docs + examples updated
- [ ] unit tests for core + codec + runtime
- [ ] integration coverage for each Phase 7 item
- [ ] at least one E2E scenario per item
- [ ] explicit semantics + exclusions documented
- [ ] `make ci-local`
