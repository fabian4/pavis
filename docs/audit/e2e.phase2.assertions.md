# E2E Test Audit - Phase 2: Assertions & Oracles

- Audit Phase: Phase 2 (Assertions & Oracles)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T06:17:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Assertion Inventory

The suite uses the following mechanisms to validate system state:

### 1.1 HTTP Response Assertions
- **Status Codes**: `assert_status` (in `tests/lib/assert.sh`) checks for expected outcomes like `200 OK` or `409 Conflict`.
- **Body Content**: `assert_body` (in `tests/lib/assert.sh`) verifies the `instance_id` field from `pavis-mock-upstream` to confirm correct routing targets.
- **JSON Field extraction**: `assert_json_has_key` ensures mock responses are well-formed before detailed parsing.

### 1.2 Binary Artifact Assertions
- **Byte Equality**: `cmp -s` is used in `relay/10_contract_opaque.sh` to ensure binary-identical distribution.
- **ETag/Version Propagation**: Extraction of `x-pavis-version` headers from relay responses to verify ordering.

### 1.3 Process & System Assertions
- **PID Stability**: Capturing and comparing PIDs (e.g. in `pavis/20_reload_norestart.sh`) to prove hot-reload without restart.
- **Process Liveness**: `kill -0` checks in fallback tests to ensure rejection of a bad config doesn't crash the proxy.

## 2. Oracle Quality

- **Externally Observable**: Assertions focus primarily on network behavior and process lifecycle, which are the primary interfaces for users and operators.
- **Low Coupling**: The tests avoid checking internal memory state or log string matching for correctness criteria, preferring HTTP-level signals.
- **Mock-based Precision**: The use of `pavis-mock-upstream` provides a high-quality oracle for traffic verification, returning structured JSON that describes how Pavis reached the backend.

## 3. False-Positive Risk

### 3.1 Startup Racing
In `relay/30_fanout_multi.sh`, the test assumes subscribers are ready after a 2s sleep. If they are not ready, the publish event might be missed, but the test might fail ambiguously rather than asserting readiness. (Remediated recently via metrics-based polling).

### 3.2 Health Check Transparency
The `wait_for_url "/health"` helper indicates the process is listening, but doesn't necessarily prove internal sub-systems (like the configuration agent) are active. If an assertion follows immediately, it might fail purely due to initialization delay rather than functional bugs.

### 3.3 PID Recycling
The PID stability check (`pavis/20_reload_norestart.sh`) has a theoretical risk of passing if a process restarts so quickly it receives the same PID from the OS, though this is statistically negligible in a controlled test environment.
