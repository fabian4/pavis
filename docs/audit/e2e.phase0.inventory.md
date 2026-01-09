# Audit Phase 0: Inventory & Test Topology
**Target Module:** E2E
**Timestamp:** 2026-01-09T12:00:00Z
**AI Model:** gemini-2.0-flash-exp

## 1. Test Inventory

The E2E testing landscape is primarily composed of shell script suites located in the root `tests/` directory.

### Locations & Counts
| Location | Type | Count | Description |
| :--- | :--- | :--- | :--- |
| `tests/suites/pavis/` | Runtime | 20 | Functional tests for the Pavis runtime (routing, headers, TLS). |
| `tests/suites/relay/` | Control Plane | 16 | Tests for the Relay service (ingest, validation, persistence). |
| `tests/suites/integrated/` | System | 14 | Integration scenarios involving both Runtime and Relay (updates, recovery). |
| `crates/pavis-e2e/` | Rust E2E | 0 | Directory exists but contains no active test files or `Cargo.toml`. |

**Total Tests:** 50

### Entry Points
- **Primary Orchestrator:** `tests/run.sh`
  - Runs all suites or specific suites/cases.
  - Handles build steps (`cargo build --release`) if in binary mode.
- **Harness Library:** `tests/lib/`
  - `harness.sh`: Environment setup/teardown.
  - `deploy.sh`: Component lifecycle management.
  - `network.sh`: Port selection and readiness waiting.
  - `assert.sh`: Verification helpers.

## 2. Test Topology

The E2E framework supports two deployment modes controlled by `TEST_MODE`: `binary` (default) and `docker`.

### Components
- **Runtime:** `pavis` binary (or container).
- **Control Plane:** `pavis-relay` binary (or container).
- **CLI:** `pavctl` binary (used for config compilation).
- **Upstreams:** `nginx` or `minimal-server` (implied by `start_upstreams` in `run.sh`, though implementation details of upstream starting were not fully visible in initial inspection, `01_routing.sh` assumes port 8081 is available/mocked or started globally).

### Network
- **Mode:** Host networking (`127.0.0.1` for binaries, `--network host` for Docker).
- **Discovery:**
  - Ports are dynamically allocated using Python (`get_free_port`) to avoid collisions.
  - Components are configured via generated config files pointing to these specific ports.

### Data
- **Artifacts:**
  - Configs (`.yaml`) are generated per test in a temp directory.
  - Artifacts (`.pvs`) are compiled using `pavctl` before runtime startup.
- **State:**
  - Temporary directories (`tests/temp/case_timestamp/`) store logs, PIDs, and config files.
  - Isolated per test case.

## 3. Entry & Exit Conditions

### Startup
1. **Build:** `tests/run.sh` builds release binaries if needed.
2. **Setup:** Each test script sources `harness.sh`, calling `setup_test`.
3. **Environment:** Creates a unique temp directory (`TEST_TMP`).
4. **Resources:** Allocates free ports.

### Readiness
- **Polling:** Tests use `wait_for_url` (curl loop) or `wait_for_port` (nc loop) to detect when services are up.
- **Timeout:** Default timeouts are ~5-30 seconds.

### Teardown
- **Mechanism:** `trap cleanup_trap EXIT` is standard boilerplate in test scripts.
- **Actions:**
  - Kills processes recorded in `pids/*.pid`.
  - Stops containers recorded in `pids/*.container`.
  - Removes `TEST_TMP` (unless `KEEP_TMP=true`).

## 4. Observations (Non-Judgmental)
- The framework relies heavily on shell scripting and external tools (`curl`, `nc`, `python3`).
- Upstream backends seem to be shared or started globally by `run_suite` (via `start_upstreams`), rather than per-test.
- `crates/pavis-e2e` appears to be a stub or abandoned attempt at Rust-based E2E testing.
