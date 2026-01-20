# Pavis Integrated Suite: Design & Strength Review

## Executive Summary

**Overall Credibility**: SOUND (one partial case requiring explicit version header validation)

**Key Strengths**:
- Full stack validation (pavctl → relay → runtime → mock upstream)
- Mode-agnostic infrastructure (Binary + Docker)
- Relay restart resilience with automatic reconnection validated
- Idempotent update stability testing eliminates false-positive failures

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

### `20_reload_switch`

**Category**: End-to-End Reload
**Contracts**: I1, I2, I5
**Maturity**: L3

**Scenario**:
1. Start relay + pavis with V1 config (backend-v1)
2. Validate initial routing to backend-v1
3. Capture initial SUT process/container ID
4. Publish V2 config (backend-v2) to relay
5. Poll up to 20 times for traffic switch

**Oracle**:
- HTTP status and response bodies from `/echo`
- SUT process/container ID

**Assertions**:
- Traffic switches from backend-v1 to backend-v2
- SUT process ID unchanged (no restart)

**Assessment**: PASS. Proves dynamic route switch across full control plane path without runtime restart.

---

### `21_reload_stable`

**Category**: End-to-End Reload
**Contracts**: I2 (Hot Reload Pipeline)
**Maturity**: L3

**Scenario**:
1. Start relay + pavis with V1 config (backend-v1)
2. Validate traffic routes correctly
3. Publish V1 again (idempotent republish: same content, new relay version)
4. Send 20 sequential requests with 100ms delay

**Oracle**:
- HTTP status codes from 20 sequential requests
- Instance IDs from response bodies

**Assertions**:
- All 20 requests succeed (no drops)
- All 20 requests route to backend-v1 (no incorrect switches)

**Assessment**: PASS. Proves stability under redundant/idempotent updates (no false wakeups or disruptions).

---

### `30_lkg_artifact`

**Category**: Failure & LKG
**Contracts**: I3 (Artifact Opaqueness), I4 (System LKG)
**Maturity**: L2

**Scenario**:
1. Start relay + pavis with V1 config (backend-v1)
2. Validate initial traffic
3. Publish corrupt data ("CORRUPT" text) to relay
4. Wait 2s, validate traffic still on backend-v1
5. Publish valid V3 config (backend-v3 on v2 port)
6. Poll up to 20 times for switch

**Oracle**:
- HTTP status and response bodies from `/echo`

**Assertions**:
- After corrupt artifact: traffic still on backend-v1
- After valid V3: traffic switches to backend-v2 (V3 uses v2 port)

**Assessment**: PARTIAL. Validates LKG behavior but relies on fixed `sleep 2s` to assume poll happened. Missing: Explicit proof that runtime fetched bad artifact but rejected it. Needed: Assert relay version > runtime version after bad artifact publish (e.g., via version headers or debug endpoints).

---

### `31_lkg_rejection`

**Category**: Failure & LKG
**Contracts**: I4 (System LKG)
**Maturity**: L2 (once implemented)
**Status**: SKIPPED (runtime semantic validation not implemented)

**Intent**: Validate runtime-side semantic rejection of an artifact that is syntactically valid (passes relay publication) but semantically broken, ensuring strict preservation of LKG semantics.

**Semantic Error**: Route references a non-existent upstream cluster.

**Rationale**: Pure semantic error with no OS/network/timing dependencies. Detectable during config compilation before applying the artifact.

---

#### Test Flow

**Setup (Baseline):**
1. Publish valid baseline artifact with route `/test` → upstream `backend`
2. Start runtime, apply baseline (version 1)
3. Assert: Traffic routes correctly, runtime version = 1

**Publish Invalid Artifact:**
4. Publish artifact with route `/test` → upstream `nonexistent` (but upstream named `backend` exists)
5. Wait for runtime long-poll cycle

**Observe Rejection:**
6. Check logs: `"semantic validation failed: route '/test' references unknown upstream 'nonexistent'"`
7. Check runtime version: MUST remain at 1 (not advance to 2)

**Assert LKG Behavior:**
8. Traffic still succeeds using baseline route (version 1 behavior)
9. Relay version = 2, Runtime version = 1 (proves fetch occurred but rejection happened)

---

#### Key Assertions

**Binary Evidence:**
- ✓ Runtime fetched artifact v2 from relay (log: `"Fetched artifact version 2"`)
- ✓ Runtime rejected v2 for semantic reasons (log: `"semantic validation failed"`)
- ✓ Runtime version remains at 1 (version did NOT advance)
- ✓ Traffic continues using v1 routes (no disruption)

**Metric Evidence (optional):**
- `pavis_config_rejections_total{reason="semantic_validation"}` increments
- `pavis_config_version` remains at `1`

---

#### Why This Design is Stable

**Determinism:**
- No OS dependencies (route→upstream validation is pure graph traversal)
- No network dependencies (error detected before upstream connection attempts)
- No timing races (semantic validation is synchronous during load)

**Alignment with I4:**
- Fetch occurs (relay version advances, runtime polls)
- Rejection is explicit (semantic validation phase before apply)
- LKG preserved (runtime does NOT update config; traffic uninterrupted)

---

#### Implementation Requirement

Runtime MUST implement semantic validation phase:
```rust
fn load_and_apply_artifact(artifact: &[u8]) -> Result<()> {
    let config = verify_binary_format(artifact)?;  // Existing
    validate_semantics(&config)?;  // NEW: Check route→upstream refs
    apply_config(config)?;  // Only if both validations pass
}
```

**Alternative semantic errors** (if route→upstream is complex):
1. Duplicate listener port: `address: "0.0.0.0:18080"` appears twice
2. Listener-admin port conflict: listener port = admin port
3. Virtual host references non-existent route table

All options are pure semantic errors detectable at load time without external state.

---

**Assessment**: SKIPPED. Blocked on runtime implementing semantic validation phase. Once implemented, enable test and set maturity to L2.

---

### `32_runtime_env_rejection`

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

### `40_resilience_restart`

**Category**: Resilience
**Contracts**: I2, I4
**Maturity**: L3

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

**Assessment**: PASS. Mode-agnostic verification. Proves:
- Runtime survives relay downtime using LKG
- Runtime automatically reconnects after relay restart
- New configs propagate after recovery

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

| Category            | Cases | Maturity Distribution |
|---------------------|-------|-----------------------|
| Smoke & Bootstrap   | 1     | L3: 1                 |
| End-to-End Reload   | 2     | L3: 2                 |
| Failure & LKG       | 2     | L2: 1, SKIPPED: 1     |
| Resilience (Restart)| 1     | L3: 1                 |

**Total Cases**: 6
**L3 (Full Proof)**: 5
**L2 (Partial Proof)**: 1
**Skipped**: 1

### Risk Coverage Mapping

**High-Risk Areas** (and coverage):
- **Control plane to data plane propagation failure** (I1): L3
- **Runtime restart during reload** (I2): L3
- **Runtime accepting bad config** (I4): L2 (validation relies on timing, not explicit version mismatch)
- **Relay downtime causing data plane failure** (I4): L3

**Well-Covered Areas**:
- Full stack bootstrap (I1): L3
- Hot reload without restart (I2): L3
- Relay restart resilience (I4): L3
- Idempotent update stability (I2): L3
- Deployment parity (I5): L3 (mode-agnostic infrastructure)

**Weak or Partially Covered Areas**:
- **System LKG with explicit version validation** (I4): L2 - `30_lkg_artifact` lacks version header checks
- **Semantic config rejection** (I4): READY - test redesigned with deterministic error (route→upstream reference); blocked on runtime implementing semantic validation phase
