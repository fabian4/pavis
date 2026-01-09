# Audit Phase 3: Determinism, Timing & Flakiness Risk
**Target Module:** E2E
**Timestamp:** 2026-01-09T12:15:00Z
**AI Model:** gemini-2.0-flash-exp

## 1. Timing Controls

### Sleep Usage
- **High Risk:** `tests/suites/relay/11_rapid_toggle.sh` uses hardcoded `sleep 0.5` commands to wait for file watcher and ingest processing.
  - **Why it's bad:** On slow CI runners, 500ms might be insufficient for the `debounce_ms` (100ms) + processing time + I/O, leading to flaky failures.
- **Good Practice:** `tests/lib/network.sh` uses polling loops (`wait_for_url`, `wait_for_port`) rather than fixed sleeps for service readiness.

### Startup Races
- **Mitigated:** Services are not assumed to be ready immediately. The polling mechanism in `deploy.sh` ensures tests block until the HTTP/TCP port is open.

## 2. Environment Assumptions

### Port Allocation
- **Risk:** Medium. `get_free_port` (in `tests/lib/network.sh`) uses the "bind-print-close" pattern.
  - **Race Condition:** There is a window between the Python script closing the socket and the Pavis binary binding to it. On a busy system, another process could claim the port.
- **Concurrency:** The current `run.sh` executes tests serially, reducing collision risk. However, parallel execution would make this highly flaky.

### Tooling Dependencies
- **Implicit:** Tests assume `curl`, `nc` (netcat), and `python3` are available and behave consistently across platforms (e.g. `nc` flags vary significantly between BSD/GNU versions).
- **Docker/Host:** `deploy.sh` contains conditional logic for MacOS (`host.docker.internal`), showing awareness of platform differences, but this adds complexity and potential "works on my machine" issues.

## 3. Cleanup & Isolation

### Resource Management
- **Reliability:** High. Tests use `trap cleanup_trap EXIT` to ensure teardown occurs even on failure.
- **Mechanism:** PIDs and Container IDs are tracked in files (`tests/temp/.../pids/`).
- **Leak Risk:** Low. If the script crashes *before* writing the PID file, the process leaks.
- **State Isolation:** Each test runs in a unique, timestamped temporary directory (`tests/temp/case_timestamp`), preventing file system collisions.

## 4. Observations
- The primary source of flakiness risk is the use of `sleep` for asynchronous event consistency (e.g., config propagation).
- The port allocation strategy prevents parallel test execution.
