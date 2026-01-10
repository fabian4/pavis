# Execution Plan: E2E Test Suite Remediation

This plan addresses the risks identified in `docs/audit/e2e.audit.summary.md`.

## 1. Eliminate Race Conditions in Relay Fanout
**Target**: `tests/suites/relay/30_fanout_multi.sh`
**Problem**: Uses `sleep 2` to wait for background subscribers, which is non-deterministic.
**Fix**: Leverage the `pavis_relay_longpoll_wait_total` metric to poll for subscriber readiness.
- [ ] Update `30_fanout_multi.sh` to poll `GET /metrics`.
- [ ] Proceed with publication only when the metric reaches the expected count (5).

## 2. Implement Missing Resource Limit Enforcement
**Target**: `tests/suites/relay/70_limits_oversize.sh`
**Problem**: Case is currently skipped.
**Fix**:
- [ ] Create a `relay.yaml` with a small `max_pvs_bytes` (e.g., 512 bytes).
- [ ] Generate a larger artifact via `pavctl`.
- [ ] Assert that `POST /v1/publish` returns `413 Payload Too Large`.
- [ ] Verify the relay maintains the previous LKG state.

## 3. Implement Control-Plane Resilience Verification
**Target**: `tests/suites/integrated/40_resilience_restart.sh`
**Problem**: Case is currently skipped.
**Fix**:
- [ ] Start full path (Relay + Runtime + Upstream).
- [ ] Confirm traffic is flowing.
- [ ] Stop the Relay process.
- [ ] Assert traffic continues to flow (LKG preservation).
- [ ] Restart the Relay.
- [ ] Publish a configuration update.
- [ ] Assert the Runtime successfully reconnects and applies the update via traffic observation.

## 4. Enable Parallel Test Execution
**Target**: `tests/run.sh`
**Problem**: Tests run sequentially, leading to high CI overhead.
**Fix**: Leverage existing `TEST_TMP` and `get_free_port` isolation.
- [ ] Introduce a `MAX_PARALLEL` environment variable (default to 1 for safety).
- [ ] Update `run_suite` logic to background test cases and manage a worker pool.
- [ ] Ensure logs are correctly segregated and printed only upon completion or failure to avoid interleaved output.

## 5. (Optional) Enhance LKG Integrity Checks
**Target**: `tests/suites/pavis/31_lkg_incompatible.sh`
**Fix**:
- [ ] Implement a test case that attempts to bind to a privileged port (e.g., 1) or a duplicate listener.
- [ ] Verify that the Runtime rejects the update and stays on the valid LKG.
