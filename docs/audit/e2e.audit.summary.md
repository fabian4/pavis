# Audit Summary: E2E Testing
**Target Module:** E2E
**Timestamp:** 2026-01-09T12:30:00Z
**AI Model:** gemini-2.0-flash-exp

## 1. Verdict
**Verdict:** E2E Coverage is Partial (Gaps Exist)

The E2E suite provides a solid baseline of functional verification for the Pavis Runtime and Control Plane. It successfully validates the "Critical User Journeys" (configuration, routing, updates). However, it suffers from significant tooling deficiencies (lost logs, brittle assertions) and timing risks that undermine confidence in CI reliability and debuggability.

## 2. Top Risks

1.  **Diagnosability Black Hole (Phase 4):**
    -   **Issue:** Application logs (`stdout`/`stderr`) are deleted immediately upon test failure by default.
    -   **Impact:** Developers cannot diagnose CI failures without re-running tests with specific manual flags. This dramatically increases time-to-resolution.

2.  **Timing & Flakiness (Phase 3):**
    -   **Issue:** Tests like `relay/11_rapid_toggle.sh` use hardcoded `sleep` commands to handle asynchronous consistency.
    -   **Impact:** High probability of "flaky" failures on slower CI runners, leading to alert fatigue and ignored tests.

3.  **Brittle Assertions (Phase 2):**
    -   **Issue:** JSON responses are validated using substring matching (`grep` or bash regex) rather than structural parsing.
    -   **Impact:** Tests may pass false positives (e.g., matching a key name instead of a value) or break on harmless formatting changes.

## 3. Confidence Assessment

-   **User-Facing Behavior:** **High.** Tests accurately simulate real-world usage (CLI -> API -> Traffic), respecting architectural boundaries.
-   **Failure Modes:** **Medium.** Good coverage of configuration errors, but limited coverage of runtime crashes or partial system failures.
-   **Stability:** **Medium.** The reliance on `sleep` and lack of structured polling for some internal states introduces non-determinism.

## 4. Next Steps (Actionable)

1.  **Fix Log Preservation:**
    -   Modify `tests/lib/harness.sh` to preserve the temporary directory (`TEST_TMP`) automatically if a test returns a non-zero exit code.
    -   *Context:* Phase 4 findings.

2.  **Harden Assertions:**
    -   Introduce `jq` to the test environment (or a lightweight Python helper since Python is already required).
    -   Replace string-matching assertions in `observability.sh` and others with precise JSON value checks.
    -   *Context:* Phase 2 findings.

3.  **Eliminate Hardcoded Sleeps:**
    -   Refactor `relay/11_rapid_toggle.sh` to use a polling loop that checks the version/status endpoint until it matches the expectation or times out.
    -   *Context:* Phase 3 findings.
