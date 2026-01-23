# Pavis Integrated Suite: Design & Strength Review

## Executive Summary

**Overall Credibility**: SOUND (one partial case requiring explicit version header validation)

**Key Strengths**:
- Full stack validation (pavctl → relay → runtime → mock upstream)
- Mode-agnostic infrastructure (Binary + Docker)
- Idempotent update stability testing eliminates false-positive failures
- Deferred relay restart coverage tracked in suite (SKIP pending relay restart test)

## System Integration Contract

The Integrated Suite proves that independent components (`pavctl`, `pavis-relay`, `pavis` runtime, `pavis-mock-upstream`) function correctly as a distributed system. Configuration compiled by a user must propagate through the control plane and be applied by the data plane without downtime.

### Formal Invariants

- **I1 (End-to-End Publish)**: Valid configuration compiled by `pavctl` and published to `relay` becomes active in `pavis` within bounded time.
- **I2 (Hot Reload Pipeline)**: Runtime updates configuration via long-poll from relay without process restarts.
- **I3 (Artifact Opaqueness)**: Relay transfers artifacts regardless of content (as long as valid PVS format).
- **I4 (System LKG)**: If bad update enters relay, runtime rejects it and maintains traffic service using Last-Known-Good configuration.
- **I5 (Deployment Parity)**: Integration logic holds true whether components run as native binaries or Docker containers.

---

## Test Case Analysis

### `10_bootstrap_path`

**Category**: Smoke & Bootstrap
**Contracts**: I1 (End-to-End Publish), I2 (Hot Reload Pipeline)
**Maturity**: L3

**Scenario**:
1. Start relay with memory storage and valid `lkg_path`
2. Start pavis with minimal config (listener only, no routes) connected to relay
3. Validate initial state: `/echo` returns 404
4. Use `pavctl gen` + direct relay publish API (`POST /v1/publish`)
5. Config V1 contains listener + upstream (backend-v1) + routes
6. Poll up to 20 times (500ms intervals) for traffic to succeed

**Oracle**:
- HTTP status from `/echo`
- JSON response body with `instance_id` field

**Assertions**:
- Initial state: `/echo` returns 404
- After publish: `/echo` returns 200 with `instance_id = "backend-v1"`

**Assessment**: PASS. Proves full path: pavctl → relay → runtime → traffic, including initial empty state → active routing.

---

### `20_reload_stable`

**Category**: End-to-End Reload
**Contracts**: I2 (Hot Reload Pipeline)
**Maturity**: L3

**Scenario**:
1. Start relay + pavis with V1 config (backend-v1)
2. Validate traffic routes correctly
3. Start a 200-request burst in background and wait for the first request to start
4. Publish V1 again (idempotent republish: same content, new relay version)
5. Wait for burst completion

**Oracle**:
- HTTP status codes from 20 sequential requests
- Instance IDs from response bodies

**Assertions**:
- All requests succeed (zero drops)
- All requests route to backend-v1 (no incorrect switches)

**Assessment**: PASS. Proves stability under redundant/idempotent updates with zero-drop guarantee.

---

### `30_lkg_artifact`

**Category**: Failure & LKG
**Contracts**: I3 (Artifact Opaqueness), I4 (System LKG)
**Maturity**: L3

**Scenario**:
1. Start relay + pavis with V1 config (backend-v1)
2. Validate initial traffic
3. Publish corrupt data to relay (expect 4xx rejection)
4. Verify relay version unchanged and runtime remains on v1
5. Publish valid V3 config (backend-v3 on v2 port)
6. Poll up to 20 times for switch

**Oracle**:
- HTTP status and response bodies from `/echo`

**Assertions**:
- Corrupt publish rejected by relay (4xx)
- Relay version unchanged and runtime stays on backend-v1
- After valid V3: traffic switches to backend-v2 (V3 uses v2 port)

**Assessment**: PASS. Validates LKG behavior and relay integrity rejection without sleep-based assumptions.

---

### `31_lkg_rejection`

**Category**: Failure & LKG
**Contracts**: I4 (System LKG)
**Maturity**: L3
**Status**: ACTIVE

**Intent**: Validate runtime env rejection occurs before apply and preserves LKG while traffic continues.

---

#### Test Flow

**Setup (Baseline):**
1. Publish valid baseline artifact with route `/test` → upstream `backend`
2. Start runtime, apply baseline (version 1)
3. Assert: Traffic routes correctly, runtime version = 1

**Publish Invalid Artifact:**
4. Publish artifact with missing TLS cert/key paths
5. Wait for runtime validation failure metric/log

**Observe Rejection:**
6. Check logs/metrics for runtime env rejection
7. Check runtime version: MUST remain at 1 (not advance to 2)

**Assert LKG Behavior:**
8. Traffic still succeeds using baseline route (version 1 behavior)
9. Relay version = 2, Runtime version = 1 (proves fetch occurred but rejection happened)

---

#### Key Assertions

**Binary Evidence:**
- ✓ Runtime rejected v2 for env reasons
- ✓ Runtime version remains at 1 (version did NOT advance)
- ✓ Traffic continues using v1 routes (no disruption)

---

#### Why This Design is Stable

**Determinism:**
- No timing races (waits on validation failure metric/log)
- No semantic ambiguity (invalid TLS paths fail at env validation)

**Alignment with I4:**
- Fetch occurs (relay version advances, runtime polls)
- Rejection is explicit (runtime env validation before apply)
- LKG preserved (runtime does NOT update config; traffic uninterrupted)

---

**Assessment**: PASS. Confirms pre-apply env validation and LKG preservation under load.

---

### `40_validation_runtime_env_rejection`

**Category**: Failure & LKG
**Contracts**: I4 (System LKG)
**Maturity**: L3

**Scenario**:
1. Start relay + pavis with valid V1 (backend-v1) and metrics enabled
2. Publish V2 enabling TLS with missing cert/key paths
3. Runtime should reject V2 on env validation

**Oracle**:
- Metrics: `pavis_config_validation_total{result="fail",reason="runtime"}`
- Upstream echo (`instance_id`)

**Assertions**:
- Runtime emits runtime validation failure metric
- Traffic remains on backend-v1 (LKG)
- Runtime stays alive

**Assessment**: PASS. End-to-end proof of runtime env validation in the control/data plane pipeline.

---

### `60_resilience_restart`

**Category**: Resilience
**Contracts**: I2, I4
**Maturity**: SKIPPED

**Scenario**:
1. Start relay (memory storage) + pavis (bootstrap config: no routes)
2. Publish V1: valid config with routes to backend-v1
3. Poll until traffic succeeds
4. Kill relay via `stop_sut "relay"`
5. Send 5 sequential requests with 100ms delay (LKG validation)
6. Restart relay via `run_relay`
7. Publish V2: new config routing to backend-v2
8. Poll up to 50 times (200ms intervals) for reconnect

**Oracle**:
- HTTP status and response bodies from `/echo`
- Relay process state

**Assertions**:
- After relay kill: all 5 requests succeed (pavis continues with LKG)
- After relay restart + V2 publish: traffic switches to backend-v2 (reconnection successful)

**Assessment**: SKIPPED. Deferred until relay restart coverage is re-enabled.

---

### `50_versioning_chain`

**Category**: End-to-End Reload
**Contracts**: I2 (Monotonic), I5 (Deployment Parity)
**Maturity**: SKIPPED

**Scenario**:
1. Start relay and pavis with V1.
2. Rapidly publish V2, V3, and V4 in a chain.
3. Monitor runtime version via metrics to ensure it applies all versions in monotonic order without skipping or regressing.

**Oracle**:
- `pavis_config_version` metric sampled during storm.

**Assertions**:
- Runtime version sequence observed: `1 -> 2 -> 3 -> 4`.
- No version regressions or crashes.

**Assessment**: SKIPPED. Deferred until relay monotonic publish enforcement is implemented.

---

## Implementation Principles

- **Runner Managed Lifecycle**: Component lifecycle managed by `run.sh`; test cases use `run_pavis` / `run_relay` / `run_mock_relay`.
- **Black-Box Testing**: Assertions based on HTTP status codes and upstream `/echo` data (`instance_id` field).
- **Isolation**: Unique `X-Pavis-Test-Run` headers per case to avoid cross-contamination.
- **Mode-Agnostic Infrastructure**: Use `get_sut_id`, `stop_sut`, `check_sut_alive` for Binary and Docker modes.
- **Polling Strategy**: Use bounded retry loops (typically 20-50 retries with 100-500ms intervals) instead of fixed sleeps for propagation delays.

---

## Coverage Analysis

### Summary

| Category              | Cases | Maturity Distribution |
|-----------------------|-------|-----------------------|
| Smoke & Bootstrap     | 1     | L3: 1                 |
| End-to-End Reload     | 1     | L3: 1                 |
| Failure & LKG         | 3     | L3: 3                 |
| Resilience (Restart)  | 1     | SKIPPED               |
| Version Chain         | 1     | SKIPPED               |

**Total Cases**: 7  
**L3 (Full Proof)**: 5  
**L2 (Partial Proof)**: 0  
**Skipped**: 2

### Risk Coverage Mapping

**High-Risk Areas** (and coverage):
- **Control plane to data plane propagation failure** (I1): L3
- **Runtime accepting bad config** (I4): L3
- **Relay downtime causing data plane failure** (I4): SKIPPED (restart case deferred)

**Well-Covered Areas**:
- Full stack bootstrap (I1): L3
- Hot reload without restart (I2): L3
- Idempotent update stability (I2): L3
- Deployment parity (I5): L3 (mode-agnostic infrastructure)

**Weak or Partially Covered Areas**:
- **Relay restart resilience** (I4): SKIPPED (deferred)
- **Version chain monotonicity** (I2): SKIPPED (deferred)
