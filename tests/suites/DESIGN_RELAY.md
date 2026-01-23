# Pavis Relay Suite: Design & Strength Review

## Executive Summary

**Overall Credibility**: SOUND (minor gaps in temporal validation and sequence tracking)

**Key Strengths**:
- Metrics-based subscriber readiness polling eliminates race conditions
- Byte-level opaque verification using binary matching oracles
- Comprehensive long-poll semantics coverage including boundary conditions and false-wake prevention

## Control Plane Contract

The Relay Suite validates the **Control Plane** correctness of the `pavis-relay` binary. The relay is treated as a black-box HTTP artifact distribution engine responsible for:
1. Accepting opaque configuration artifacts via publication API
2. Distributing artifacts to subscribers using efficient long-polling semantics
3. Supporting fanout to multiple subscribers without data loss
4. Persisting state across process restarts
5. Validating artifacts and enforcing size limits
6. Maintaining strict ETag-based versioning semantics

### Formal Invariants

- **R1 (Opaque Transfer)**: Artifacts are stored and served byte-for-byte identical to published input.
- **R2 (Versioned/ETag)**: Every artifact has a unique, monotonic version and ETag (`sha256:...`).
- **R3 (Efficient Long-Poll)**: Subscribers requesting current version block until new version available or timeout expires.
- **R4 (Fanout Correctness)**: Single publication event propagates to ALL active long-polling subscribers.
- **R5 (Concurrency Safety)**: Simultaneous operations do not corrupt state or regress versions.
- **R6 (Persistence)**: Restarted relay serves Last-Known-Good (LKG) artifact immediately.
- **R7 (Backpressure)**: Relay rejects artifacts exceeding configured limits.

---

## Test Case Analysis

### `10_contract_opaque`

**Category**: Contract & Integrity
**Contracts**: R1 (Opaque Transfer), R2 (Versioned/ETag)
**Maturity**: L3

**Scenario**:
1. Start relay with in-memory storage
2. Generate minimal valid `.pvs` artifact
3. Publish via `POST /v1/publish`
4. Fetch via `GET /v1/config`

**Oracle**:
- HTTP 200 response
- ETag header format `sha256:...`
- Body bytes

**Assertions**:
- ETag matches format `^sha256:[0-9a-f]{64}$`
- Published and fetched artifacts are byte-identical (via `cmp` oracle)

**Assessment**: PASS. Binary matching provides strong opaque transfer proof.

---

### `11_contract_republish_monotonicity`

**Category**: Contract & Long-Poll Semantics
**Contracts**: R1 (Opaque Transfer), R2 (ETag), R3 (Efficient Long-Poll), R5 (No False Wake)
**Maturity**: L3

**Scenario**:
1. Publish artifact (V1)
2. Start long-poll subscriber with `If-None-Match: <etag>` and `wait_ms=3000`
3. Publish identical artifact again (idempotent republish)
4. Fetch and compare

**Oracle**:
- HTTP 200 on both publishes
- ETag values across requests
- Version headers
- Long-poll status + elapsed time

**Assertions**:
- ETag identical across republishes of same bytes
- Version increments monotonically (1 → 2)
- Fetched artifact matches original
- Long-poll completes with HTTP 204 after ~3000ms (no early wake)

**Assessment**: PASS. Proves idempotent republish semantics, monotonic versioning, and no false wakeup.

---

### `20_longpoll_wait`

**Category**: Long-Poll Semantics
**Contracts**: R3 (Efficient Long-Poll), R2 (Versioned/ETag)
**Maturity**: L2

**Scenario**:
1. Publish initial config
2. Fetch to obtain ETag
3. Issue long-poll with `wait_ms=500` and `If-None-Match: <etag>`

**Oracle**:
- HTTP 204 after timeout
- ETag header in response
- Elapsed time

**Assertions**:
- Status code = 204
- ETag unchanged
- `400ms < elapsed < 700ms`

**Assessment**: PASS. Proves timeout behavior with elapsed-time liveness check.

---

### `21_longpoll_timeout`

**Category**: Long-Poll Semantics
**Contracts**: R3 (Efficient Long-Poll)
**Maturity**: L3

**Scenario**:
1. Publish artifact
2. Long-poll with `wait_ms=2000` and matching ETag
3. No new publish event occurs

**Oracle**:
- HTTP 204 after timeout
- Elapsed time

**Assertions**:
- Status code = 204
- `elapsed >= 1800ms`

**Assessment**: PASS. Temporal oracle proves blocking behavior.

---

### `12_contract_etag_validation`

**Category**: Long-Poll Semantics
**Contracts**: R2 (Versioned/ETag)
**Maturity**: L3

**Scenario**:
1. Publish config
2. Test multiple invalid ETag formats:
   - Weak ETags (`W/"..."`)
   - Wildcard (`*`)
   - Multiple ETags (`"etag1", "etag2"`)
   - Malformed prefix (`"md5:..."`)
   - Short hex values

**Oracle**:
- HTTP status codes for each invalid format

**Assertions**:
- All invalid ETags ignored → HTTP 200 (unconditional GET)
- No long-poll blocking on malformed ETags

**Assessment**: PASS. Strict validation of `sha256:...` format; all non-compliant formats rejected.

---

### `30_fanout_multi`

**Category**: Fanout
**Contracts**: R4 (Fanout Correctness)
**Maturity**: L3

**Scenario**:
1. Publish V1
2. Spawn 5 background long-poll subscribers with same ETag
3. Poll `pavis_relay_longpoll_wait_total` metric until count >= 5
4. Publish V2
5. Wait for all subscribers to complete

**Oracle**:
- Prometheus metrics (`pavis_relay_longpoll_wait_total`)
- HTTP status codes from background processes

**Assertions**:
- All 5 subscribers registered before V2 publish
- All 5 received HTTP 200

**Assessment**: PASS. Metrics-based readiness polling eliminates race conditions.

---

### `31_fanout_late`

**Category**: Fanout
**Contracts**: R2 (Versioned/ETag)
**Maturity**: L3

**Scenario**:
1. Publish V1, fetch ETag1
2. Publish V2, V3, V4, V5 rapidly (subscriber falls behind)
3. Late subscriber requests with ETag1

**Oracle**:
- HTTP status code
- Latency

**Assertions**:
- HTTP 200 immediately (non-blocking catch-up)
- `latency < 2000ms`

**Assessment**: PASS. Proves non-blocking catch-up for stale clients.

---

### `40_concurrency_rapid`

**Category**: Concurrency
**Contracts**: R5 (Concurrency Safety), R2 (Versioned/ETag)
**Maturity**: L2

**Scenario**:
1. Generate 50 unique `.pvs` artifacts
2. Spawn publisher loop (publishes all 50 in rapid succession)
3. Spawn subscriber loop (long-polls 100 times with ETag tracking)
4. Track version headers during polling

**Oracle**:
- Version headers from responses
- Final state version
- Health endpoint

**Assertions**:
- Version never regresses during sequence
- Final version = 50
- Relay survives (health check passes)

**Assessment**: PARTIAL. Validates final state (version 50) and detects regressions when observed, but only checks versions actually seen during polling loop. Missing: explicit logging and validation that ALL observed version headers form a strictly monotonic sequence (no gaps in verification).

---

### `50_persistence_recovery`

**Category**: Persistence
**Contracts**: R6 (Persistence)
**Maturity**: L3

**Scenario**:
1. Start relay with file-based storage
2. Publish artifact
3. Fetch and validate
4. Stop relay via `stop_sut`
5. Start new relay instance with same storage directory
6. Fetch artifact again

**Oracle**:
- HTTP status codes
- Body bytes across restarts

**Assertions**:
- Fetched artifact after restart is byte-identical to original

**Assessment**: PASS. Mode-agnostic verification (Binary + Docker).

---

### `13_contract_transport_integrity`

**Category**: Contract & Integrity
**Contracts**: R1 (Opaque Transfer), R2 (Versioned/ETag)
**Maturity**: L3

**Scenario**:
1. Publish artifact
2. Fetch and inspect HTTP headers and body

**Oracle**:
- HTTP headers: `Content-Type`, `ETag`, `X-Config-Size`, `Cache-Control`
- Body: magic bytes, size

**Assertions**:
- `Content-Type: application/octet-stream`
- `ETag: sha256:...` (format validation)
- `X-Config-Size` matches actual body size
- `Cache-Control: no-store`
- Body non-empty
- Magic bytes = `50415653` ("PAVS")

**Assessment**: PASS. Protocol-level transport integrity verified.

---

### `61_robustness_reconnect`

**Category**: Robustness
**Contracts**: R2 (Versioned/ETag), R3 (Efficient Long-Poll)
**Maturity**: L3

**Scenario**:
1. Publish V1, fetch ETag1
2. Start long-poll, kill mid-request (simulates disconnect)
3. Publish V2
4. Reconnect and request with ETag1

**Oracle**:
- HTTP status code
- Body bytes
- Latency

**Assertions**:
- HTTP 200 with V2 body
- Immediate return (`< 2000ms`, non-blocking)

**Assessment**: PASS. Disconnection does not corrupt state; late subscriber catch-up works.

---

### `60_robustness_boundary_conditions`

**Category**: Long-Poll Semantics
**Contracts**: R3 (Efficient Long-Poll), R2 (Versioned/ETag)
**Maturity**: L3

**Scenario**:
Tests edge cases:
1. `wait_ms=0` with matching ETag
2. `wait_ms=70000` (exceeds max 60000)
3. Missing `If-None-Match` + `wait_ms > 0`
4. `wait_ms=60000` (max) with matching ETag

**Oracle**:
- HTTP status codes
- Latency for each scenario

**Assertions**:
1. `wait_ms=0` + matching ETag → HTTP 304
2. `wait_ms=70000` → HTTP 400
3. Missing `If-None-Match` + `wait_ms > 0` → HTTP 200 immediately (`< 500ms`)
4. `wait_ms=60000` + matching ETag → HTTP 204 after ~60s (full CI only)

**Assessment**: PASS. Comprehensive boundary testing with CI profile gating.

---

### `70_limits_oversize`

**Category**: Limits
**Contracts**: R7 (Backpressure)
**Maturity**: L3

**Scenario**:
1. Start relay with `max_pvs_bytes: 100`
2. Generate artifact > 100 bytes
3. Publish

**Oracle**:
- HTTP status code

**Assertions**:
- HTTP 413 (Payload Too Large)

**Assessment**: PASS. Rejects oversized artifacts.

---

### `71_limits_empty`

**Category**: Limits
**Contracts**: R1 (Opaque Transfer)
**Maturity**: L3

**Scenario**:
1. Publish empty body

**Oracle**:
- HTTP status code

**Assertions**:
- HTTP 400 or 422 (rejection)

**Assessment**: PASS. Rejects empty/invalid artifacts.

---

## Implementation Principles

- **Isolation**: Each case runs against a fresh, isolated relay instance.
- **Determinism**: Wait for readiness via `/health` or `/status` before executing logic.
- **Black-Box Testing**: Interact solely via HTTP API and observable process state.
- **Metrics-Based Readiness**: Use Prometheus metrics (`pavis_relay_longpoll_wait_total`) to verify subscriber registration.
- **Mode-Agnostic Infrastructure**: Use `stop_sut` and `run_relay` for Binary and Docker modes.

---

## Coverage Analysis

### Summary

| Category               | Cases | Maturity Distribution |
|------------------------|-------|-----------------------|
| Contract & Integrity   | 4     | L3: 4                 |
| Long-Poll Semantics    | 2     | L3: 2                 |
| Fanout                 | 2     | L3: 2                 |
| Concurrency            | 1     | L2: 1                 |
| Persistence            | 1     | L3: 1                 |
| Robustness             | 2     | L3: 2                 |
| Limits                 | 2     | L3: 2                 |

**Total Cases**: 14
**L3 (Full Proof)**: 13
**L2 (Partial Proof)**: 2
**L1 (Sanity)**: 0

### Risk Coverage Mapping

**High-Risk Areas** (and coverage):
- **State corruption under concurrency** (R5): L2 (final state only, not full sequence)
- **Long-poll false wakeups** (R3, R5): L3 (republish stability test)
- **ETag validation bypass** (R2): L3 (comprehensive boundary testing)
- **Data corruption during transfer** (R1): L3 (byte-level verification)

**Well-Covered Areas**:
- Opaque transfer semantics (R1): L3
- Persistence across restarts (R6): L3
- Fanout correctness (R4): L3
- Backpressure/limits (R7): L3
- Transport protocol integrity: L3

**Weak or Partially Covered Areas**:
- **Version monotonicity during concurrent updates** (R5): L2 - `40_concurrency_rapid` checks final state only
