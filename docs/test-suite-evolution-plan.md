# Test Suite Evolution Plan

This document consolidates unfinished work items from the suite DESIGN docs.

## Integrated Suite

### Gaps
- `30_lkg_artifact` relies on fixed sleep instead of explicit version validation.
- `31_lkg_rejection` is blocked on runtime semantic validation.

### Short-Term (Must Address)
1. `30_lkg_artifact` enhancement:
   - Add relay version check (e.g., `/v1/config` or `/v1/status`).
   - Add runtime version check (explicit version header or equivalent).
   - Assert relay version > runtime version after bad artifact publish.
2. `31_lkg_rejection` enabling:
   - Implement semantic validation phase in runtime (route→upstream reference or duplicate listener port).
   - Ensure validation happens before `apply_config()`.

### Mid-Term (Should Improve)
3. Concurrent traffic during reload:
   - Add burst testing during `20_reload_switch`.
   - Validate zero-drop semantics under load.

### Long-Term (Optional Enhancements)
4. pavctl integration testing (explicit binary tests for version flag, errors, exit codes).
5. Network partition simulation (iptables/pf) and recovery validation.
6. Relay failover during long-poll.

## Pavis Runtime Suite

### Gaps
- TLS/mTLS coverage blocked by rustls backend (7 cases).
- Timeout/retry policies, access log flush/sync timing, and tracing remain unimplemented (3 cases).

### Short-Term (Must Address)
1. Stabilize TLS/mTLS coverage:
   - Migrate to a TLS backend with per-peer CA and client cert support.
2. Implement timeout/retry policies in runtime.
3. Resolve access log flush/sync timing issues.

### Mid-Term (Should Improve)
4. Add negative resilience tests:
   - Outlier detection with partial failures.
   - Circuit breaker recovery after backoff.

### Long-Term (Optional Enhancements)
5. Weighted routing with probabilistic splits (large sample sizes).
6. Concurrent reload stress test (V1 → V10 under sustained traffic).

## Relay Suite

### Gaps
- `20_longpoll_wait` lacks explicit subscriber liveness proof.
- `40_concurrency_rapid` validates only final state, not full monotonic sequence.

### Short-Term (Must Address)
1. `20_longpoll_wait` enhancement:
   - Add subscriber process liveness check before publish.
   - Prove blocking behavior (no immediate 204).
2. `40_concurrency_rapid` instrumentation:
   - Log all observed version headers during polling.
   - Assert strict monotonicity across observed versions.

### Mid-Term (Should Improve)
3. Metrics consistency validation:
   - Cross-check `pavis_relay_longpoll_wait_total` vs. subscriber count.
4. Persistence edge cases:
   - Relay restart while subscribers are waiting.
   - Validate reconnection behavior.

### Long-Term (Optional Enhancements)
5. ETag collision handling (theoretical).
6. Backpressure under sustained publish load.
