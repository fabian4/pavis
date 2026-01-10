# E2E Case Strength Review

## 0) Executive Summary
- **Total Cases Scanned**: 26
- **Status Breakdown**: 
    - ✅ **Solid**: 17 cases (Clear assertions, stable synchronization).
    - ⚠️ **Needs Expansion**: 6 cases (Assertions are correct but superficial).
    - 🧨 **Brittle-Risk**: 3 cases (Relies on race-prone background timings or fragile header parsing).

### Top 3 Cross-Cutting Improvements
1.  **Fragile Header Parsing**: Many scripts use `grep | awk` to extract `x-pavis-version` or `ETag`. This often fails if headers change case or order. Propose a `get_header` library helper.
2.  **Weak Reload Proofs**: Reload tests verify that traffic *eventually* switches, but they don't verify **Zero-Drop** invariants during the transition window.
3.  **Ambiguous Result Aggregation**: Cases using background subshells (especially in `relay`) sometimes lose error details, leading to "Success by default" if `wait` logic is misconfigured.

---

## 1) Cross-Cutting Issues & Shared Fixes

### Issue: Fragile Header Extraction
- **Symptom**: `version=$(echo "$response" | grep -i "x-pavis-version:" | awk '{print $2}' | tr -d '\r')` repeated in multiple scripts.
- **Risk**: Hard to maintain; sensitive to whitespace and multi-line response formatting.
- **Fix**: Add `get_header` to `tests/lib/assert.sh`.
```bash
get_header() {
    local header_name=$1
    # Extracts value of a header from a raw HTTP response (curl -i)
    grep -Fi "$header_name:" | awk -v FS=': ' '{print $2}' | tr -d '[:space:]'
}
```

### Issue: Unprotected Traffic Generation
- **Symptom**: Direct `curl` calls often forget `X-Pavis-Test-Run` headers, risking cross-test pollution in mock-upstream.
- **Risk**: Flaky tests when running in parallel or high-load environments.
- **Fix**: Add `pavis_curl` wrapper to `tests/lib/env.sh`.
```bash
pavis_curl() {
    local path=$1; shift
    curl -s -H "X-Pavis-Test-Run: ${RUN_ID}" -H "X-Pavis-Test-Case: ${CASE_NAME}" "$@" "$path"
}
```

---

## 2) Suite: pavis (runtime)

### `20_reload_norestart`
- **Current Coverage**: Verifies traffic eventually hits V2; verifies PID is stable.
- **Gaps**: Does not prove that **zero** requests failed during the actual swap moment.
- **Proposed Expansion**: Use a background loop to send 50 requests/sec during the `publish_config` call. 
- **Assertion**: Assert that every single request in the burst returns `200 OK`.
- **Justification**: Zero-downtime is a core Pavis claim; current test only proves "eventual consistency" of the reload.

### `41_traffic_weighted`
- **Current Coverage**: Checks that both backends are hit eventually.
- **Gaps**: Sample size is too small to differentiate from a bug in the LB algorithm (e.g., Round Robin vs Weighted).
- **Proposed Expansion**: Send 40 requests.
- **Assertion**: For a 50/50 split, assert that neither backend receives `> 30` requests.
- **Justification**: Distinguishes "both work" from "balancing is actually applied".

### `60_security_tls`
- **Current Coverage**: Verifies `tls.enabled` in echo response.
- **Gaps**: Does not verify that SNI was actually sent to the upstream.
- **Proposed Expansion**: Check the `tls.sni` field in the mock-upstream `/echo` response.
- **Assertion**: Assert `tls.sni == "localhost"` (or as configured).
- **Justification**: Proves origination logic correctly populates handshake metadata.

---

## 3) Suite: relay

### `20_longpoll_wait`
- **Current Coverage**: Background subscriber returns when data is published.
- **Gaps**: Doesn't verify the subscriber **actually blocked**. If the relay is buggy and returns immediately, the test might still pass.
- **Proposed Expansion**: Check duration of the background `curl`.
- **Assertion**: Assert `DURATION >= 1s` (since the script sleeps 1s before publishing).
- **Justification**: Ensures the "Long" part of Long-Polling is functioning.

### `40_concurrency_rapid`
- **Current Coverage**: Checks final version is 50.
- **Gaps**: Intermediate states are ignored. A bug where versions skip or go backwards might be missed.
- **Proposed Expansion**: Collect version headers from the subscriber loop.
- **Assertion**: Assert each new version seen is `>=` previous version.
- **Justification**: Enforces version monotonicity from the client perspective during stress.

---

## 4) Suite: integrated

### `10_bootstrap_path`
- **Current Coverage**: Full path bootstrap.
- **Gaps**: Minimal.
- **Proposed Expansion**: No change needed.

### `30_lkg_artifact`
- **Current Coverage**: Corrupt artifact doesn't break traffic.
- **Gaps**: Relies on `sleep 2` to assume the poll cycle finished.
- **Proposed Expansion**: Poll the Relay metrics `/metrics` for a "fetch" attempt.
- **Assertion**: Wait for `pavis_relay_longpoll_wait_total` to increment (indicating Pavis re-connected after the rejection).
- **Justification**: Removes the "magic number" sleep and proves the runtime is still healthy enough to re-poll.

### `40_resilience_restart`
- **Current Coverage**: Relay restart recovery.
- **Gaps**: Doesn't prove Pavis tried to reconnect during the outage (backoff logic).
- **Proposed Expansion**: Capture Pavis logs during the outage period (optional debug) OR check `/received` on upstream.
- **Assertion**: No expansion needed; existing assertions are strong.

---

## 5) Recommended Implementation Plan

### Phase 1: Shared Helpers (High Impact)
- Add `get_header`, `assert_http_status`, and `pavis_curl` to `tests/lib/`.
- Update all cases to use `pavis_curl` to eliminate isolation header boilerplate.

### Phase 2: Zero-Drop Invariants
- Expand `pavis/20_reload_norestart` and `integrated/21_reload_stable` with the traffic burst logic.
- Target: Prove 100.0% success rate during reloads.

### Phase 3: Deterministic Relay Proofs
- Update `relay/20_longpoll_wait` with timing assertions.
- Refine `integrated/30_lkg_artifact` to use metric-based polling instead of `sleep 2`.

### Phase 4: Standardize
- Replace all remaining `[ $status -eq 0 ]` with named assertion helpers for readability.
- Standardize all `SKIP` reasons across the suite.
