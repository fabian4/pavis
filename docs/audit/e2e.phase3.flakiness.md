# E2E Test Audit - Phase 3: Determinism, Timing & Flakiness Risk

- Audit Phase: Phase 3 (Determinism, Timing & Flakiness Risk)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T06:18:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Timing Controls

### 1.1 Use of Sleep
The suite contains significant usage of `sleep` for synchronization:
- **Polling Intervals**: `sleep 0.1` and `sleep 0.5` are used within polling loops (e.g. `pavis/20_reload_norestart.sh`). This is generally acceptable.
- **Fixed Waits**: `pavis/30_lkg_corrupt.sh` uses `sleep 2` to wait for a poll cycle. This is a flakiness risk if the test environment is highly loaded or the default poll interval is increased.
- **Race conditions**: Fixed sleeps used to wait for background jobs (e.g. `relay/30_fanout_multi.sh` - recently updated to poll metrics instead).

### 1.2 Timeouts
- `wait_for_url` and `wait_for_port` use fixed timeouts (30s and 10s respectively). While sufficient for local execution, these might be tight for heavy CI environments with limited CPU.

## 2. Environment Assumptions

### 2.1 Port Allocation
- The suite uses `get_free_port` which binds a temporary socket via Python to find available ports. This is a highly deterministic and parallel-safe strategy.

### 2.2 Filesystem Paths
- Every test uses a timestamped `${TEST_TMP}` directory. This ensures complete isolation of artifacts and logs between concurrent or subsequent runs.

### 2.3 Docker Networking
- In Docker mode, tests assume `127.0.0.1` interaction via `--network host`. On macOS, the helper `get_host_addr` abstracts the switch to `host.docker.internal`.

## 3. Cleanup & Isolation

### 3.1 Resource Cleanup
- **`trap cleanup_test EXIT`**: Standard across all tests. It kills all PIDs in `${TEST_TMP}/pids/*.pid`.
- **Ghost Processes**: If a script is killed via `SIGKILL` (e.g. by CI executor), the trap might not fire, leaving orphaned binaries.

### 3.2 State Isolation
- **Isolation Headers**: Mandatory use of `X-Pavis-Test-Run` and `X-Pavis-Test-Case` ensures that `pavis-mock-upstream` can distinguish traffic between concurrent tests. This is a very robust design.
