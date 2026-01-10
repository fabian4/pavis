# E2E Test Audit - Phase 0: Inventory & Test Topology

- Audit Phase: Phase 0 (Inventory & Test Topology)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T05:15:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Test Inventory

The Pavis E2E test suite is organized into three primary categories, executed by a central runner (`tests/run.sh`).

### 1.1 Runtime E2E (`tests/suites/pavis/`)
Focuses on the `pavis` data plane binary, specifically its lifecycle and traffic handling capabilities.
- `10_bootstrap_static.sh`: Validates initial startup from a local artifact.
- `20_reload_norestart.sh`: Verifies hot-reload via long-poll without process restart.
- `30_lkg_corrupt.sh`: Checks fallback to Last-Known-Good configuration upon receiving corrupt artifacts.
- `31_lkg_incompatible.sh`: Validates rejection of semantically invalid artifacts.
- `40_traffic_matcher.sh`: Verifies routing logic changes under hot-reload.
- `41_traffic_weighted.sh`: Validates weighted traffic shifting.
- `50_resilience_timeout.sh`: (Skipped) Planned timeout policy validation.
- `51_resilience_retry.sh`: (Skipped) Planned retry policy validation.
- `60_security_tls.sh`: Validates TLS origination toggling.

### 1.2 Relay / Control-Plane E2E (`tests/suites/relay/`)
Validates the `pavis-relay` binary as an artifact distribution engine.
- `10_contract_opaque.sh`: Basic publish/subscribe of opaque artifacts.
- `11_contract_republish.sh`: Monotonicity and idempotency checks.
- `20_longpoll_wait.sh`: Efficient blocking updates.
- `21_longpoll_timeout.sh`: Timeout behavior for unchanged state.
- `30_fanout_multi.sh`: Distribution to multiple concurrent subscribers.
- `31_fanout_late.sh`: Late subscriber catch-up behavior.
- `40_concurrency_rapid.sh`: High-frequency monotonic updates.
- `50_persistence_recovery.sh`: State recovery across relay restarts.
- `60_robustness_reconnect.sh`: Subscriber disconnection and reconnection.
- `70_limits_oversize.sh`: (Skipped) Payload size limit enforcement.
- `71_limits_empty.sh`: Handling of empty payload publications.

### 1.3 Integrated System E2E (`tests/suites/integrated/`)
Validates the full path: `pavctl` -> `pavis-relay` -> `pavis` runtime -> `pavis-mock-upstream`.
- `10_bootstrap_path.sh`: End-to-end bootstrap via relay.
- `20_reload_switch.sh`: System-wide traffic shift proof.
- `21_reload_stable.sh`: Stability during redundant updates.
- `30_lkg_artifact.sh`: System-wide preservation of LKG.
- `31_lkg_rejection.sh`: (Skipped) Semantic rejection in integrated context.
- `40_resilience_restart.sh`: (Skipped) Relay restart recovery for runtimes.

## 2. Test Topology

The system topology is managed by `tests/lib/docker.sh` and `tests/lib/env.sh`, supporting two distinct modes.

### 2.1 Component Roles
- **Publisher**: `pavctl` or `curl` (mocking publisher) sending artifacts to Relay.
- **Control Plane**: `pavis-relay` (production binary) or `pavis-mock-relay` (test fixture).
- **Data Plane**: `pavis` (production binary).
- **Backend**: `pavis-mock-upstream` (test fixture, multiple instances).

### 2.2 Network & Process Management
- **Binary Mode (`TEST_MODE=binary`)**:
    - Services are launched as background processes on the host.
    - Coordination via dynamically allocated free ports (e.g., `get_free_port` in `tests/lib/env.sh`).
    - PIDs are recorded in `$TEST_TMP/pids/` for lifecycle management.
- **Docker Mode (`TEST_MODE=docker`)**:
    - Infrastructure services (`backend-v1`, `backend-v2`) are managed via `docker compose`.
    - SUT components (`pavis`, `pavis-relay`) are launched via `docker run --network host`.
    - Components interact via `127.0.0.1`, effectively mirroring the binary mode topology within the host namespace.

## 3. Entry & Exit Conditions

### 3.1 Startup & Readiness
- **Entry**: `tests/run.sh` triggers suites. Each case script sources `env.sh` and calls `setup_test`.
- **Readiness**:
    - Upstreams: `wait_for_port` (binary) or `docker compose up --wait` (docker).
    - SUT: Explicit `wait_for_url` against `/health` or `/healthz` endpoints.
- **Isolation**: Every test case uses a unique `RUN_ID` and `CASE_NAME`, transmitted via `X-Pavis-Test-Run` and `X-Pavis-Test-Case` headers for state isolation in the mock upstream.

### 3.2 Termination & Cleanup
- **Termination**: Each script uses a `trap cleanup_test EXIT`.
- **Cleanup**: 
    - `cleanup_test` kills all PIDs in the case-specific PID directory and stops recorded containers.
    - `tests/run.sh` performs a final `rm -rf tests/temp` unless `KEEP_TMP=true` is set.
    - Upstream certificates are cleared via `cleanup_certs`.