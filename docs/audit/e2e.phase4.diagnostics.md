# E2E Test Audit - Phase 4: Failure Diagnostics & Debuggability

- Audit Phase: Phase 4 (Failure Diagnostics & Debuggability)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T05:35:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Failure Signal Quality

The suite provides excellent diagnostic signals through automated log management and artifact preservation.

### 1.1 Automated Log Dumps
The `run_case` function in `tests/run.sh` captures the entire execution log of each test script. Upon failure, this log is immediately printed to stdout under a `❌ Failure Details` header. 

Crucially, the `cleanup_test` function in `tests/lib/env.sh` performs a recursive grep across all SUT logs (`$TEST_TMP/logs/`) if a test exits with a non-zero code. This ensures that the root cause (e.g., a Pavis panic or a Relay storage error) is visible in the CI console without requiring manual artifact retrieval.

### 1.2 Artifact Preservation
The system uses unique, timestamped temporary directories (`TEST_TMP`) for every test case.
- **On Success**: The directory is deleted (unless `KEEP_TMP=true`).
- **On Failure**: The directory is preserved, and its path is printed. This allows developers to inspect generated `.pvs` artifacts, hex-edit them for local debugging, and review the exact YAML configuration used during the failed run.

### 1.3 Execution Mode Parity
The ability to switch between `binary` and `docker` modes using the `TEST_MODE` environment variable allows developers to reproduce complex containerized failures using local binaries for faster iteration and easier attaching of debuggers (e.g., `gdb`, `lldb`).

## 2. Signal-to-Noise Ratio

### 2.1 Ambiguity in High-Level Assertions
Some failure messages are relatively generic:
- `❌ Traffic did not start flowing after publish` (`integrated/10_bootstrap_path.sh`)
- `❌ LKG failed` (`pavis/30_lkg_corrupt.sh`)

While the subsequent log dump usually clarifies whether the failure was due to a network timeout, a binary rejection, or an upstream error, the script-level error message itself does not always pinpoint the component at fault.

### 2.2 Shell Error Handling
The use of `set -e` ensures that any command failure (like a failed `curl` or a mismatched `cmp`) immediately terminates the test and triggers the cleanup/log-dump flow. This prevents "ghost passes" where a test continues after a critical failure.

## 3. Reproducibility

Reproducibility is rated **Very High**:
- **Single Case Execution**: The runner explicitly supports executing individual tests: `./tests/run.sh <suite> <case_name>`.
- **Environment Isolation**: The use of `get_free_port` and `TEST_TMP` minimizes the impact of host state on test outcomes, making local results highly consistent with CI results.