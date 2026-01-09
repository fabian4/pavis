# Audit Phase 5: E2E Scope & Cost Balance
**Target Module:** E2E
**Timestamp:** 2026-01-09T12:25:00Z
**AI Model:** gemini-2.0-flash-exp

## 1. Scope Adequacy

### Functional Coverage
- **Verdict:** Good. The suite covers the "Critical User Journeys": defining config, applying it, routing traffic, and updating it.
- **Appropriateness:** Tests focus on the integration of components (CLI, Relay, Pavis) which is the correct scope for E2E. They do not excessively test internal logic that should be unit tested (e.g., complex regex matching is tested minimally to ensure the engine is wired up).

### Misnomers
- **"Stress" Tests:** `tests/suites/pavis/10_stress_routing.sh` is a misnomer. It performs 50 sequential requests. This verifies basic stability but provides zero value as a load or stress test.

## 2. Redundancy

### Vs. Unit Tests
- **Minimal:** While `12_wildcard_host.sh` tests routing logic that is likely also unit tested, doing so via `curl` verifies the HTTP Host header parsing and the entire request path, which unit tests cannot mock perfectly. This overlap is healthy.

### Vs. Benchmarks
- **Separation:** The existence of a top-level `bench/` directory (observed in Phase 0) suggests that true performance testing is offloaded there. The E2E suite correctly stays focused on correctness, not speed.

## 3. Cost Signals

### Execution Speed
- **Fast:** The tests use lightweight local processes. The heaviest operation is `docker compose` for the shared upstream, which is done once per suite.
- **Resource Usage:** Low. Running ~50 sequential tests is very cheap.

### Parallelization Limits
- **Bottleneck:** The suite is currently serial. The port allocation strategy (Phase 3) prevents safe parallel execution. As the suite grows, this will become a bottleneck.

## 4. Missing Critical Scenarios
- **Traffic Interruption:** While there are update tests, there is no explicit test verifying "Zero Downtime" during a reload (e.g., running a load generator *during* a config reload and asserting 0 failures).
- **Soak Testing:** No tests run for longer than a few seconds. Memory leaks or resource exhaustion over time are not covered.
