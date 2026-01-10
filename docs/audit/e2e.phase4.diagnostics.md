# E2E Test Audit - Phase 4: Failure Diagnostics & Debuggability

- Audit Phase: Phase 4 (Failure Diagnostics & Debuggability)
- Target Module: E2E
- Generation Timestamp: 2026-01-10T06:19:00Z
- AI Model Identifier: Gemini 2.0 Flash

## 1. Failure Signal Quality

### 1.1 Automatic Log Dumps
- **`run_case` in `tests/run.sh`**: Captures stdout/stderr of every test script to a log file. On failure, it immediately dumps this log to the terminal using `cat`.
- **SUT Logs**: `cleanup_test` in `tests/lib/env.sh` performs a recursive grep across the `${TEST_TMP}/logs/` directory if the exit code is non-zero. This provides immediate visibility into SUT panics or startup errors.

### 1.2 Preservation of Artifacts
- On failure, `${TEST_TMP}` is preserved. It contains:
  - `logs/`: Full output from all services.
  - `pids/`: ID files for orphaned process detection.
  - Case-specific generated `.pvs` and YAML files.
- The path to this folder is printed clearly at the end of a failed run.

## 2. Reproducibility

- **High Reproducibility**: Since every test case is a standalone shell script, developers can execute a single failed test by running `bash tests/suites/<suite>/<case>.sh` or using the runner: `./tests/run.sh <suite> <case_name>`.
- **Mode Toggle**: The ability to switch between `binary` and `docker` modes allows debugging environment-specific issues (like port binding differences) easily.

## 3. Signal-to-Noise Ratio

- **Assertion Clarity**: Most failures use the `❌` marker and print the expected vs actual values (e.g. `assert_status`).
- **Binary mode vs Docker**: Binary mode failures are extremely clear as they provide direct access to the process logs on the host filesystem. Docker mode failures are also clear because logs are mounted from the host into the container.
