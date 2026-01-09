# Audit Phase 4: Failure Diagnostics & Debuggability
**Target Module:** E2E
**Timestamp:** 2026-01-09T12:20:00Z
**AI Model:** gemini-2.0-flash-exp

## 1. Failure Signal Quality

### Log Availability (Critical Defect)
- **Problem:** By default, all test artifacts—including component logs (`stderr`/`stdout` of Pavis and Relay)—are deleted immediately after the test finishes.
- **Mechanism:** `tests/lib/harness.sh` defines `cleanup_test`, which runs `rm -rf "$TEST_TMP"` unless `KEEP_TMP=true`.
- **Impact:** When a test fails in CI or locally, the developer receives only the assertion error (e.g., "Status was 500") but loses the application logs that explain *why* (e.g., "Panic: index out of bounds").

### Console Output
- **Clarity:** Test scripts print specific failure messages (e.g., `echo "❌ Assertion failed..."`). This provides good immediate feedback on *what* failed.
- **Verbosity:** The harness does not automatically stream application logs to the console on failure.

## 2. Reproducibility

### Local Execution
- **Ease of Use:** High. Running `bash tests/run.sh` is straightforward.
- **Environment:** The ability to switch between `binary` and `docker` modes helps isolate issues related to the runtime environment vs. the binary itself.

### Signal-to-Noise
- **Ambiguity:** Without the logs, failure signals are highly ambiguous. A "Connection Refused" error could mean the binary crashed, failed to start due to config, or timed out. Differentiating these requires re-running with `KEEP_TMP=true`.

## 3. Recommendations (For Report Context Only)
- The harness urgently needs a "fail-safe" mechanism where `TEST_TMP` is preserved *automatically* if the test function returns a non-zero exit code, regardless of the `KEEP_TMP` setting.
- Logs should ideally be tailed to stdout upon failure to provide immediate context in CI logs.
