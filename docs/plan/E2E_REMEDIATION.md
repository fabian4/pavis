# Execution Plan: E2E Test Suite Remediation (Revised & Hardened)

This plan addresses systemic flakiness and assertion weaknesses by prioritizing deterministic, black-box signals and enforcing the "Recoverability" and "Zero-Drop" invariants of the Frozen Data Plane.

## Phase 1: Shared Infrastructure & Isolation
- [x] **Robust Header Extraction**
  - Description: Replace fragile `grep | awk` pipelines with a case-insensitive, whitespace-trimmed helper.
  - Implementation Hint: Implement `header_value(file, name)` using `sed` to extract values from `curl -i` output files.
- [x] **Traffic Wrapper Split**
  - Description: Separate body and header capture to avoid shell variable corruption from binary artifacts.
  - Implementation Hint: Provide `pavis_curl_body` (output to stdout) and `pavis_curl_headers` (output to file) ensuring `X-Pavis-Test-Run` is always present.
- [x] **Assertion Library Expansion**
  - Description: Add helpers for status codes and header equality to reduce boilerplate.
  - Implementation Hint: Implement `assert_header_eq(file, name, value)` and `assert_status_eq(file, expected)`.

## Phase 2: Zero-Drop & Temporal Hardening
- [x] **Zero-Drop Reload Proof (`pavis/20_reload_norestart`)**
  - Description: Prove configuration updates cause zero failed requests and exactly one atomic switch.
  - Implementation Hint: Run a tight `while` loop sending 100 requests during the `publish` call; assert 100% status 200 and verify version string in body transitions exactly once from V1 to V2.
- [x] **Liveness-Based Blocking Proof (`relay/20_longpoll_wait`)**
  - Description: Prove the subscriber blocks until an event occurs without relying on clock time.
  - Implementation Hint: Start background subscriber; verify `kill -0 $SUB_PID` is true *before* publishing; publish update; `wait $SUB_PID` and assert exit code 0.
- [x] **Monotonicity Verification (`relay/40_concurrency_rapid`)**
  - Description: Ensure subscribers never see a version regression during high-frequency updates.
  - Implementation Hint: Capture headers from 50 sequential requests; assert `version[N] >= version[N-1]` using a numeric comparison loop.

## Phase 3: LKG Recoverability & Security Depth
- [x] **LKG Recoverability Proof (`pavis/30_lkg_corrupt`)**
  - Description: Prove that a rejected configuration does not leave the runtime in a terminal or "stuck" state.
  - Implementation Hint: Publish Corrupt V2 (Assert traffic stays on V1) -> Publish Valid V3 (Assert traffic switches to V3).
- [x] **Integrated LKG Switch (`integrated/30_lkg_artifact`)**
  - Description: Validate system-wide LKG preservation using behavioral signals.
  - Implementation Hint: Publish Corrupt V2 -> Assert `/echo` still returns Backend V1 metadata -> Publish Valid V3 -> Assert Backend V3 metadata.
- [ ] **Handshake Validation (`pavis/60_security_tls`)**
  - Description: Prove TLS origination is not just "on" but correctly configured.
  - Implementation Hint: Query mock-upstream `/echo` and assert the `tls.sni` field in the JSON body matches the configured hostname. (Note: Currently limited by mock-upstream lack of SNI detection).

## Phase 4: Load-Balancing & Policy Validation
- [x] **Deterministic Weight Flip (`pavis/41_traffic_weighted`)**
  - Description: Verify weighted load balancing without statistical flakiness.
  - Implementation Hint: Test 100/0 split (Assert all hit A) -> Update to 0/100 split (Assert all hit B) -> Update to 50/50 and assert both are hit within 100 requests.
- [ ] **Timeout Enforcement (`pavis/50_resilience_timeout`)**
  - Description: Verify data-plane enforcement of latency SLAs.
  - Implementation Hint: Map `/hang` on upstream; set 100ms timeout; assert Pavis returns 504 Gateway Timeout immediately.

## Completed / Re-evaluate Section
- [x] **Race-Free Fanout (`relay/30_fanout_multi`)**
  - Re-evaluate if metrics are unstable: Uses `pavis_relay_longpoll_wait_total` to determine subscriber readiness.
- [x] **Resource Limit Enforcement (`relay/70_limits_oversize`)**
  - Verified 413 rejection for artifacts exceeding `max_pvs_bytes`.
- [x] **Control-Plane Outage Resilience (`integrated/40_resilience_restart`)**
  - Proved data-plane continues serving LKG when relay is offline and reconnects upon relay recovery.
- [x] **Environment Agnosticism**
  - Standardized `stop_sut` and `check_sut_alive` helpers.
