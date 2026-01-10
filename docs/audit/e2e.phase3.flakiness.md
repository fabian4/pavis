# E2E Test Audit - Phase 3: Determinism, Timing & Flakiness Risk

- Audit Phase: Phase 3 (Determinism, Timing & Flakiness Risk)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T05:30:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Timing Controls

The test suites rely on a combination of event polling and fixed-duration sleeps.

### 1.1 Fixed Sleeps (High Risk)
Several tests use `sleep` as a substitute for event-based synchronization:
- **LKG Validation**: `pavis/30_lkg_corrupt.sh` and `integrated/30_lkg_artifact.sh` use `sleep 2` to wait for a poll cycle. This is a flakiness risk if system load increases or the polling interval is configured higher.
- **Background Readiness**: `relay/30_fanout_multi.sh` uses `sleep 2` to "wait for [subscribers] to be ready". This is a classic race condition; if the background `curl` processes take longer to initialize, the subsequent publish event will be missed by those subscribers.
- **Reload Grace**: Many tests (e.g., `pavis/20_reload_norestart.sh`) use `sleep 0.5` inside loops. While safe, it contributes to overall suite duration.

### 1.2 Polling & Bounded Waits
- `wait_for_url` and `wait_for_port` in `tests/lib/assert.sh` are used for readiness checks. These are deterministic but rely on arbitrary 30s and 10s timeouts. If a binary build or container start takes longer, the test will fail superficially.

## 2. Environment Assumptions

### 2.1 Port Allocation
The use of `get_free_port` (via `python3` socket bind) in case scripts is a robust strategy for avoiding port collisions during parallel execution in `binary` mode.

### 2.2 Host Interaction (`--network host`)
In `docker` mode, `tests/lib/env.sh` uses `--network host` for SUT containers. 
- **Linux**: Works as expected, allowing `127.0.0.1` interaction.
- **macOS/Darwin**: Relies on `get_host_addr` to return `host.docker.internal`. There is a risk of inconsistency if some components use `127.0.0.1` hardcoded while others use the helper.

### 2.3 Filesystem Isolation
The use of `TEST_TMP` (unique timestamped directories) ensures that concurrent tests do not interfere with each other's configuration files, PVS artifacts, or persistence stores (e.g., in `relay/50_persistence_recovery.sh`).

## 3. Cleanup & Isolation

### 3.1 State Isolation
The mandatory inclusion of `X-Pavis-Test-Run` and `X-Pavis-Test-Case` headers in all traffic allows the `pavis-mock-upstream` to segregate request counters and history. This is a high-confidence mechanism for parallel execution.

### 3.2 Resource Leakage
- **Process Leaks**: `trap cleanup_test EXIT` is present in all scripts, which attempts to kill recorded PIDs. However, if a script is killed via `SIGKILL`, the trap will not fire, potentially leaving orphan processes.
- **Container Leaks**: `cleanup_test` stops containers by ID, which is safer than `docker compose down` for parallel runs.
- **Temp Files**: `tests/run.sh` clears `tests/temp` at the end of the run, preventing disk bloat in CI environments.