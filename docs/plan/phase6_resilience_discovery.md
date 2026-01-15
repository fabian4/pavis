# Phase 6 Plan: Resilience & Discovery

**Status**: Draft
**Owner**: Codex
**Scope**: Phase 6 items in ROADMAP.md (Outlier Detection, Circuit Breaking, Active Health Checks)
**Non-Goals**: DNS discovery (already complete), cross-crate architecture changes, new crates, policy expansion beyond the Frozen Data Plane safe set

## 1. Objectives (from ROADMAP.md)
- **Outlier Detection**: Passive health checks (consecutive error ejection).
- **Circuit Breaking**: Hard caps on connections and pending requests.
- **Active Health Checks**: Minimal periodic `/healthz` probes.

## 2. Constraints & Architecture Guardrails
- **No `RuntimeConfig` struct changes without explicit approval** (must be requested before implementation).
- **Zero-Option policy** in `pavis-core` and `pavis` runtime: use explicit enums, avoid `Option<T>` for policy toggles.
- **Layering**: Config types and semantics live in `pavis-core`; codec handles defaults; runtime executes only validated config; `pavis-pvs` only validates binary integrity.
- **No new crates**.
- **Frozen-safe exclusions (explicit)**: no queueing, fairness, priority, retry budgets, sliding windows, statistical models, adaptive tuning, or external control-plane feedback.
- **State lifetime rule**: all Phase 6 runtime state is ephemeral and MUST NOT outlive the current RuntimeConfig version; clear all state on hot reload.

## 3. Decision Gates (Requires Explicit Approval)
1) **RuntimeConfig surface changes**: adding new enums/fields for circuit breaking and health checks.
2) **Behavioral defaults**: must be materialized in codecs (not runtime).
3) **Failure semantics**: fail-close vs fail-open for new policies must be documented before implementation.

## 4. Proposed Work Plan (Pending Approval of Gates)

### 4.1 Core API + Validation (pavis-core)
- **Add types** (new enums) for:
  - `OutlierDetectionPolicy`:
    - `Disabled`
    - `Enabled { consecutive_errors, eject_duration }`
  - `CircuitBreakerPolicy`:
    - `Disabled`
    - `Enabled { max_connections, max_pending_requests }`
  - `ActiveHealthCheck`:
    - `Disabled`
    - `Enabled { path, interval, timeout }`
- **Integrate into** upstream pool-level structures only (no route/listener policies).
- **Validation**:
  - Enforce bounds and invariants (interval > 0, timeout > 0, timeout <= interval, consecutive_errors > 0, eject_duration > 0).
  - Enforce breaker bounds (max_connections > 0, max_pending_requests > 0 when enabled).
  - Validate health check path syntax (starts with '/', no spaces).
- **Files** (expected):
  - `crates/pavis-core/src/runtime/*.rs`
  - `crates/pavis-core/src/validate/*.rs`

### 4.2 Codec Defaults & Conversion (pavis-codec-*)
- **Materialize defaults** for new policies (explicit Disabled/Enabled).
- **Reject unsupported fields** early (no ignored options; fail fast in codecs).
- **Files** (expected):
  - `crates/pavis-codec-serde/src/config/types.rs`
  - `crates/pavis-codec-serde/src/config/convert/*.rs`
  - `crates/pavis-codec-api/src/*` (error mapping if needed)

### 4.3 Runtime Execution (pavis)
- **Outlier detection**:
  - Track consecutive failures per endpoint and eject for `eject_duration` (no deletion).
  - Define what counts as an error (transport + optional 5xx; must be documented).
  - Re-admit after duration without half-open probing (health checks handle readiness).
- **Circuit breaking**:
  - Hard caps on `max_connections` and `max_pending_requests`; exceed -> immediate 503.
  - Define scope: per-upstream caps + per-endpoint health only.
  - Define counters precisely (in-flight connections vs pooled total; pending == waiting on permit).
  - Stable error class; no implicit retries.
- **Active health checks**:
  - Periodic GET probes to `path`, no body.
  - Define Host header behavior (explicitly set or omitted).
  - Success criteria: must be documented (200-only or 2xx).
  - Bounded scheduling: per-upstream fanout limits; jitter allowed but bounded.
  - Binary pass/fail; updates endpoint health only.
- **Files** (expected):
  - `crates/pavis/src/upstream/*`
  - `crates/pavis/src/agent/*` (if needed for refresh loops)
  - `crates/pavis/src/proxy/service.rs` (enforce per-request limits)

### 4.4 PVS Boundary (pavis-pvs)
- **No semantic changes**. Ensure binary layout remains valid after core changes.
- Update docs only if serialization changes occur.

### 4.5 Tests & Verification
- **Unit tests**:
  - `pavis-core`: validation for new policies.
  - `pavis-codec-serde`: default materialization and rejection cases.
  - `pavis`: health check / outlier / breaker logic.
- **Integration/E2E**:
  - Add suites covering health check transitions, breaker caps, and outlier ejection.
  - Use existing relay + proxy harness in `tests/suites`.
- **Commands**:
  - `make ci-local`
  - `make e2e-binary` (or targeted suites)

## 5. Risks & Mitigations
- **Config churn**: new fields may ripple across codecs and tests. Mitigate with explicit approval gate and staged changes.
- **Runtime complexity**: keep policies bounded; no queues, windows, or statistical models.
- **Performance**: health checks add background work; must be bounded by config and use async timers.

## 6. Pre-Phase Gate (Strongly Recommended)
### Phase 3.7: Release Hardening
- Finalize PVS versioning policy.
- Define fail-close vs fail-open behavior for new policies.
- Document failure semantics for health, outlier, and breaker logic.

## 7. Documentation Deliverables
- Policy definitions with exact semantics and exclusions.
- Failure semantics (fail-close/open) for each policy.
- Boundedness guarantees (state reset on reload).
- Examples (YAML -> compiled config expectations).
- Update ROADMAP Phase 6 item names when merged.
- Update `docs/FEATURES.md` matrix statuses for Circuit Breaking, Active Health Checks, Outlier Detection.

## 8. Examples
- **Example A**: Circuit breaking only (max_connections/max_pending_requests) with expected 503 behavior.
- **Example B**: Active health check only (`/healthz`) with endpoint health transitions.
- **Example C**: Outlier detection only (consecutive_errors + eject_duration) with ejection + re-admission.

## 9. Tests
### Unit Tests
- `pavis-core` validation:
  - interval > 0, timeout > 0, timeout <= interval
  - consecutive_errors > 0, eject_duration > 0
  - max_connections/max_pending_requests bounds (non-zero if enabled)
  - health check path syntax
- `pavis-codec-serde`:
  - defaults materialized explicitly (Disabled/Enabled)
  - reject unsupported/unknown fields (fail fast)
  - round-trip stability if conversion exists
- `pavis` runtime:
  - breaker permit acquisition/release correctness
  - pending limit -> immediate 503
  - outlier consecutive error counter increments/reset
  - ejection timer expiry re-admits endpoint
  - health check scheduler respects interval/timeout and updates health state
  - hot reload resets all Phase 6 state

### Integration Tests
- Breaker cap -> 503 rate correlates with cap.
- Health check marks endpoint unhealthy; LB stops selecting it.
- Outlier ejection after N failures; endpoint returns after eject_duration.
- Reload config resets state and applies new policy atomically.

### E2E Tests
- Breaker cap exceeded -> 503; verify behavior end-to-end.
- Health check flips from 200 to 500; routing shifts within bounded time.
- Outlier ejection on consecutive 5xx/transport errors; re-admit after duration.
- Hot reload changes breaker limits; state reset matches docs.

## 10. Deliverables
- Core API + validation updates for Phase 6 policies.
- Codec materialization of defaults and enforcement of blocked/invalid settings.
- Runtime support for outlier detection, circuit breaking, and active health checks.
- Tests and updated docs.

## 11. Verification Checklist
- [ ] docs + examples updated
- [ ] unit tests for core + codec + runtime
- [ ] integration coverage per policy
- [ ] at least one end-to-end scenario per policy
- [ ] explicit semantics + exclusions documented
- [ ] `make ci-local`
