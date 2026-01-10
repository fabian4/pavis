# E2E Test Audit - Phase 0: Inventory & Test Topology

- Audit Phase: Phase 0 (Inventory & Test Topology)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T06:15:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Test Inventory

The Pavis E2E test suite is composed of 26 test scripts across three specialized suites, orchestrated by a central runner (`tests/run.sh`).

### 1.1 Runtime E2E (`tests/suites/pavis/`)
Focuses on the `pavis` data plane binary behavior and hot-reload.
- `10_bootstrap_static.sh`: Initial bootstrap from local artifact.
- `20_reload_norestart.sh`: Hot reload via long-poll from mock relay.
- `30_lkg_corrupt.sh`: Fallback to LKG upon receiving corrupt binary artifacts.
- `31_lkg_incompatible.sh`: Rejection of semantically invalid artifacts.
- `40_traffic_matcher.sh`: Routing logic evolution under reload.
- `41_traffic_weighted.sh`: Weighted traffic shift between backends.
- `50_resilience_timeout.sh`: (Skipped) Timeout policy enforcement.
- `51_resilience_retry.sh`: (Skipped) Retry policy enforcement.
- `60_security_tls.sh`: TLS origination toggle.

### 1.2 Relay / Control-Plane E2E (`tests/suites/relay/`)
Focuses on the `pavis-relay` distribution logic.
- `10_contract_opaque.sh`: Basic binary publish/subscribe.
- `11_contract_republish.sh`: Monotonicity enforcement.
- `20_longpoll_wait.sh`: Long-poll blocking behavior.
- `21_longpoll_timeout.sh`: Long-poll timeout logic.
- `30_fanout_multi.sh`: Broadast to multiple subscribers.
- `31_fanout_late.sh`: Late subscriber catch-up.
- `40_concurrency_rapid.sh`: High-frequency updates stress.
- `50_persistence_recovery.sh`: State recovery across restarts.
- `60_robustness_reconnect.sh`: Connection resilience.
- `70_limits_oversize.sh`: Payload size enforcement.
- `71_limits_empty.sh`: Handling of zero-byte updates.

### 1.3 Integrated System E2E (`tests/suites/integrated/`)
Focuses on the full system path: `pavctl` -> `relay` -> `pavis` -> `upstream`.
- `10_bootstrap_path.sh`: Full path bootstrap verification.
- `20_reload_switch.sh`: Dynamic routing across the system.
- `21_reload_stable.sh`: Stability under redundant updates.
- `30_lkg_artifact.sh`: Cross-component LKG preservation.
- `31_lkg_rejection.sh`: (Skipped) Integrated semantic rejection.
- `40_resilience_restart.sh`: Recovery after relay failure.

## 2. Topology Description

The suite supports two execution modes: **Binary** (native processes) and **Docker**.

- **Processes started**: 
  - `pavis`: The data plane runtime.
  - `pavis-relay`: The production control plane.
  - `pavis-mock-relay`: A test fixture for isolated runtime testing.
  - `pavis-mock-upstream`: A deterministic backend for assertions.
- **Network Topology**:
  - Components communicate via `127.0.0.1` (Binary mode) or `host.docker.internal` / container aliases (Docker mode).
  - Ports are dynamically allocated via `get_free_port` to allow parallel-safe execution.
- **Dependencies**:
  - `bash`, `curl`, `nc`, `python3` (for JSON/port utils).
  - `cargo` (for binary mode builds).
  - `docker-compose` (for containerized mode).

## 3. Entry & Exit Conditions

- **Startup**:
  - `tests/run.sh` builds release binaries (binary mode).
  - `setup_test` creates a unique `${TEST_TMP}` directory.
  - SUT readiness is determined via `wait_for_url` (polling `/health` or `/healthz`).
- **Termination**:
  - `trap cleanup_test EXIT` ensures cleanup regardless of test outcome.
  - `cleanup_test` kills recorded PIDs and stops recorded containers.
  - `tests/run.sh` clears the shared `tests/temp` folder after the run.
