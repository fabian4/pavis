# E2E Test Audit - Phase 2: Assertions & Oracles

- Audit Phase: Phase 2 (Assertions & Oracles)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T05:25:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Assertion Inventory

The E2E tests utilize a mix of black-box network assertions and process-level state checks.

### 1.1 Network Observations (HTTP)
- **Status Codes**: `assert_status` is used to verify API outcomes (e.g., `200 OK` for valid publish, `409 Conflict` for monotonicity violation in `relay/11_contract_republish.sh`).
- **Body Content**: `assert_body` verifies routing targets by checking the `instance_id` returned by `pavis-mock-upstream` (e.g., `pavis/40_traffic_matcher.sh`).
- **JSON Structure**: `assert_json_has_key` ensures mock services return the expected schema before specific values are parsed.

### 1.2 Binary & Artifact Integrity
- **Byte Comparison**: `cmp -s` is used in `relay/10_contract_opaque.sh` to prove that the relay serves exactly the same bytes it received, establishing it as a pure distribution engine.
- **Checksums**: Headers like `x-pavis-checksum` are observed in several tests to ensure metadata propagation.

### 1.3 Process Lifecycle & Invariants
- **PID Stability**: Tests like `pavis/20_reload_norestart.sh` capture the runtime PID and compare it post-reload to guarantee zero-downtime evolution.
- **Process Liveness**: `kill -0 $PID` is used in fallback tests (`pavis/30_lkg_corrupt.sh`) to ensure the proxy remains operational after rejecting a bad update.

### 1.4 Temporal Oracles
- **Request Duration**: `relay/21_longpoll_timeout.sh` measures the time taken for a `304 Not Modified` response to verify that long-polling actually blocks for the requested `wait_ms` interval.

## 2. Oracle Quality

The oracles used in the suite are generally of **high quality** due to their focus on external effects:
- **Externally Observable**: Assertions focus on HTTP responses and network connectivity, which are the primary interfaces for users and operators.
- **Decoupled from Implementation**: The tests do not assert on internal variable states or specific log message strings for correctness (logs are used only as debug aids in failure details).
- **Strong Typing via Mocks**: By using `pavis-mock-upstream`, the tests can assert on structured JSON rather than trying to parse cleartext payloads from arbitrary backends.

## 3. False-Positive Risk

The following false-positive risks were identified:

### 3.1 PID Reuse
In `binary` mode, if a process crashes and restarts so quickly that it receives the same PID from the OS (unlikely but possible), the "no-restart" assertion in `norestart` tests could pass even though a restart occurred.

### 3.2 Weak Health Checks
In some tests, `wait_for_url` against `/health` or `/healthz` is the only signal of readiness. If these endpoints return `200 OK` before the internal routing table or relay state is fully synchronized, subsequent assertions might fail or, worse, pass if they are not specific enough about the expected state version.

### 3.3 Default Routing Pass
If a routing test asserts `200 OK` but doesn't check the `instance_id` or specific backend markers, it might pass if Pavis is routing to *any* healthy backend, rather than the *specific* one targeted by the update. This risk is largely mitigated in the current suite by consistent use of `instance_id` checks.