# E2E Test Suite Remediation Plan

**Created**: 2026-01-23
**Based On**: E2E Test Case Audit Report v1.0
**Status**: Ready for Execution
**Total Items**: 58 tasks across 4 phases

---

## Executive Summary

This plan addresses **58 issues** identified in the E2E test suite audit, organized into **4 phases** by severity and type:

| Phase | Focus | Items | Blocking | Est. Effort |
|-------|-------|-------|----------|-------------|
| **Phase 0** | S0 Critical Blockers | 2 | Production | 3-5 days |
| **Phase 1** | S1 High Priority | 4 | Pre-production | 2-3 days |
| **Phase 2** | S2 Medium Priority | 10 | Quality improvements | 3-4 days |
| **Phase 3** | S3 Low Priority / Cleanup | 42 | Nice-to-have | 5-7 days |

**Critical Path**: Phase 0 must complete before production use. Phase 1 recommended before beta.

---

## Phase 0: S0 Critical Blockers (Production Blockers)

**Goal**: Fix issues that would cause production failures or data loss.
**Timeline**: Complete ASAP (3-5 days)
**Blockers**: These MUST be fixed before production deployment.

### 0.1 Runtime Bug: Agent Polling Recovery (CRITICAL)

**Issue**: `pavis/30_lkg.sh` SKIP - Runtime agent fails to recover after rejecting invalid configs
**Impact**: Production blocker - runtime gets stuck after config rejection sequence
**Root Cause**: Agent tracks `last_version` and retries 404s for rejected intermediate versions

**Tasks**:
- [ ] **0.1.1** Read `docs/plan/latest-driven-runtime.md` specification
- [ ] **0.1.2** Implement latest-driven polling logic in runtime agent
  - [ ] Change from `last_version` tracking to `latest_available` polling
  - [ ] Reset state after validation failures
  - [ ] Skip intermediate versions (don't retry 404s)
- [ ] **0.1.3** Add unit tests for agent polling state machine
- [ ] **0.1.4** Remove SKIP from `tests/suites/pavis/30_lkg.sh`
- [ ] **0.1.5** Run `make e2e-pavis CASE=30_lkg.sh` - verify PASS
- [ ] **0.1.6** Run full `make e2e` suite - verify no regressions

**Files to Modify**:
- Runtime agent polling worker (likely `pavis/src/config/agent.rs` or similar)
- `tests/suites/pavis/30_lkg.sh` (remove SKIP)

**Verification**:
```bash
make e2e-pavis CASE=30_lkg.sh
# Expected: ✅ PASS
```

---

### 0.2 Test Determinism: Sleep-Based Waits (CRITICAL)

**Issue**: 34 test cases use `sleep N && assert` causing non-deterministic failures
**Impact**: Flaky tests in CI, false failures on slow systems, undermines test reliability
**Solution**: Replace all `sleep` with bounded `wait_for_condition` helpers

**Tasks**:
- [ ] **0.2.1** Create test helper library in `tests/scripts/wait_helpers.sh`
  - [ ] `wait_for_url <url> <timeout>` - poll URL until 200 or timeout
  - [ ] `wait_for_log <pattern> <logfile> <timeout>` - grep log until match or timeout
  - [ ] `wait_for_metric <metric> <condition> <timeout>` - poll Prometheus endpoint
  - [ ] `wait_for_version <expected_version> <timeout>` - poll admin API
- [ ] **0.2.2** Fix all 34 cases with sleep-based waits (see detailed list below)
- [ ] **0.2.3** Add timeout validation after all retry loops (12 cases)
- [ ] **0.2.4** Run full `make e2e` suite 3 times - verify no flakes

**Affected Test Cases** (34 total):

**Bootstrap (2 cases)**:
- [ ] `integrated/10_bootstrap_path.sh` - line 78: `sleep 2` → `wait_for_url`
- [ ] `integrated/10_bootstrap_path.sh` - lines 85-92: add timeout check after retry loop

**Hot Reload (4 cases)**:
- [ ] `integrated/20_reload_switch.sh` - line 65: `sleep 3` → `wait_for_version`
- [ ] `integrated/21_reload_stable.sh` - line 88: `sleep 5` → barrier pattern
- [ ] `pavis/22_reload_storm.sh` - line 102: `sleep 0.5` → version polling
- [ ] `pavis/23_reload_keepalive_atomic.sh` - line 78: `sleep 1` → deterministic curl

**LKG (1 case)**:
- [ ] `pavis/32_lkg_relay_unavailable.sh` - line 72: `sleep 5` → `wait_for_log`

**Routing (1 case)**:
- [ ] `pavis/40_traffic_routing_semantics.sh` - lines 88-94: add timeout check after retry loop

**Resilience (2 cases)**:
- [ ] `pavis/53_resilience_active_health_check.sh` - line 95: `sleep 3` → probe counter polling
- [ ] `pavis/94_retry_idempotency.sh` - line 108: `sleep 2` → backend counter polling

**Observability (1 case)**:
- [ ] `pavis/70_obs_consistency.sh` - line 88: `sleep 2` → `wait_for_metric`

**Relay Protocol (1 case)**:
- [ ] `relay/20_longpoll_wait.sh` - line 92: timing tolerance ±100ms → ±200ms

**Files to Create**:
- `tests/scripts/wait_helpers.sh` (new helper library)

**Verification**:
```bash
# Run 3 times to detect flakes
for i in 1 2 3; do make e2e; done
# Expected: All runs PASS with consistent timing
```

---

## Phase 1: S1 High Priority (Pre-Production)

**Goal**: Fix architectural violations and high-severity bugs
**Timeline**: Complete before beta release (2-3 days)
**Blockers**: Recommended before production, required for architectural integrity

### 1.1 Runtime Bug: Env Validation Before Apply

**Issue**: `integrated/31_lkg_rejection.sh` SKIP - Runtime applies config before env validation
**Impact**: Violates A2 (Immutable Execution State), potential for partial application
**Root Cause**: Runtime performs lazy TLS cert loading after `swap_state()`

**Tasks**:
- [ ] **1.1.1** Move TLS cert readability checks to pre-apply phase
- [ ] **1.1.2** Change order: `validate_env() → swap_state()` (not `swap_state() → lazy_load_certs()`)
- [ ] **1.1.3** Add unit tests for validation ordering
- [ ] **1.1.4** Remove SKIP from `tests/suites/integrated/31_lkg_rejection.sh`
- [ ] **1.1.5** Verify test now passes synchronously
- [ ] **1.1.6** Run `make e2e-integrated CASE=31_lkg_rejection.sh`

**Files to Modify**:
- Runtime config apply logic (likely `pavis/src/config/apply.rs` or similar)
- `tests/suites/integrated/31_lkg_rejection.sh` (remove SKIP)

---

### 1.2 Clarify Validation Layering Contract

**Issue**: `integrated/30_lkg_artifact.sh` SKIP - Unclear if relay should validate PVS magic/checksum
**Impact**: Ambiguous layering contract, potential security gap
**Solution**: Define explicit validation boundaries

**Tasks**:
- [ ] **1.2.1** Document validation layering in `ARCHITECTURE.md`:
  - Relay: Validates magic bytes + checksum (integrity only)
  - Core: Validates semantic invariants (routing tree, regex safety)
  - Runtime: Validates environment (file paths, ports)
- [ ] **1.2.2** Implement relay-side PVS magic/checksum validation
- [ ] **1.2.3** Update `tests/suites/integrated/30_lkg_artifact.sh`:
  - Test: Send corrupt PVS → Relay rejects with 422 → Runtime keeps LKG
  - Remove SKIP
- [ ] **1.2.4** Run `make e2e-integrated CASE=30_lkg_artifact.sh`

**Files to Modify**:
- `ARCHITECTURE.md` (add validation layering section)
- Relay publish endpoint (add magic/checksum validation)
- `tests/suites/integrated/30_lkg_artifact.sh` (update expectations, remove SKIP)

---

### 1.3 Fix Unbounded Retry Loops

**Issue**: 12 test cases have retry loops without timeout validation
**Impact**: Tests hang indefinitely if condition never met

**Tasks**:
- [ ] **1.3.1** Add timeout validation pattern to `tests/scripts/assert.sh`:
  ```bash
  assert_retry_succeeded() {
    local attempt=$1
    local max_retries=$2
    [[ $attempt -lt $max_retries ]] || fail "Retry timeout after ${max_retries} attempts"
  }
  ```
- [ ] **1.3.2** Fix all 12 cases (add timeout check after each retry loop)

**Affected Cases** (12 total):
- [ ] `integrated/10_bootstrap_path.sh` - lines 85-92
- [ ] `pavis/40_traffic_routing_semantics.sh` - lines 88-94
- [ ] (10 additional cases - audit for all `while` and `for` loops with retries)

---

### 1.4 Split Layering Violation in pavis/33

**Issue**: `pavis/33_semantic_validation_suite.sh` tests both codec and runtime validation
**Impact**: Violates A3 (Layered Validation), confuses responsibility boundaries

**Tasks**:
- [ ] **1.4.1** Read `tests/suites/pavis/33_semantic_validation_suite.sh`
- [ ] **1.4.2** Identify codec-specific tests (regex syntax, YAML parsing)
- [ ] **1.4.3** Create `crates/pavis-codec-serde/tests/validation_tests.rs` for codec tests
- [ ] **1.4.4** Create `tests/suites/pavis/33_core_validation_suite.sh` for core tests (upstream refs, routing tree)
- [ ] **1.4.5** Delete old `pavis/33_semantic_validation_suite.sh`
- [ ] **1.4.6** Run `make e2e-pavis CASE=33_core_validation_suite.sh`
- [ ] **1.4.7** Run `cargo test -p pavis-codec-serde validation_tests`

**Files to Create**:
- `crates/pavis-codec-serde/tests/validation_tests.rs` (new unit tests)
- `tests/suites/pavis/33_core_validation_suite.sh` (new E2E test)

**Files to Delete**:
- `tests/suites/pavis/33_semantic_validation_suite.sh` (old mixed test)

---

## Phase 2: S2 Medium Priority (Quality Improvements)

**Goal**: Add missing coverage for architectural invariants
**Timeline**: Complete within 1-2 weeks (3-4 days)
**Blockers**: Recommended for production confidence

### 2.1 Add Missing Atomicity Test (A4 Gap)

**Issue**: No test for partial config corruption
**Impact**: A4 (Atomic Validity) not fully verified

**Tasks**:
- [ ] **2.1.1** Create `tests/suites/pavis/35_atomicity_partial_corruption.sh`
- [ ] **2.1.2** Test case: Send PVS with valid header but corrupted routes section
- [ ] **2.1.3** Verify runtime rejects 100% (not partial application)
- [ ] **2.1.4** Run `make e2e-pavis CASE=35_atomicity_partial_corruption.sh`

---

### 2.2 Add Missing Relay Opacity Test (A5 Gap)

**Issue**: No negative test for relay tampering
**Impact**: A5 (Relay Opacity) not fully verified

**Tasks**:
- [ ] **2.2.1** Update `tests/suites/relay/10_contract_opaque.sh`
- [ ] **2.2.2** Add negative test section:
  ```bash
  # Tamper with PVS bytes (flip bit in payload)
  TAMPERED=$(echo "$PVS_BYTES" | sed 's/PAVS/TAVS/')
  publish_config "http://127.0.0.1:$PORT_RELAY" "$TAMPERED"
  # Runtime should detect checksum mismatch
  assert_log_contains "checksum mismatch"
  ```
- [ ] **2.2.3** Run `make e2e-relay CASE=10_contract_opaque.sh`

---

### 2.3 Fix Test Assertions (Missing Coverage)

**Tasks**:

- [ ] **2.3.1** `relay/11_contract_republish.sh` - Add ETag assertions
- [ ] **2.3.2** `integrated/20_reload_switch.sh` - Merge into pavis/20 (DELETE)
- [ ] **2.3.3** `integrated/21_reload_stable.sh` - Add zero-drop assertion
- [ ] **2.3.4** `pavis/22_reload_storm.sh` - Add version tracking + memory assertion
- [ ] **2.3.5** `pavis/23_reload_keepalive_atomic.sh` - Verify connection reuse
- [ ] **2.3.6** `pavis/24_atomic_mid_request.sh` - Use slow backend + verify old state
- [ ] **2.3.7** `pavis/92_operational_reload_resource_sanity.sh` - Document threshold
- [ ] **2.3.8** `integrated/32_runtime_env_rejection.sh` - Add traffic continuity test
- [ ] **2.3.9** `pavis/34_runtime_env_rejection.sh` - Add serving state assertion
- [ ] **2.3.10** `pavis/41_traffic_weighted.sh` - Increase sample size to 1000

---

## Phase 3: S3 Low Priority / Cleanup

**Goal**: Clean up redundancy, naming, and minor issues
**Timeline**: Complete over 2-3 weeks (5-7 days)
**Blockers**: Nice-to-have, not blocking production

### 3.1 File Renames (8 files)

**Tasks**:
- [ ] **3.1.1** `pavis/70_obs_consistency.sh` → `pavis/70_observability_consistency.sh`
- [ ] **3.1.2** `pavis/80_pool_hard_limit.sh` → `pavis/80_connection_pool_hard_limit.sh`
- [ ] **3.1.3** `pavis/81_pool_queue_behavior.sh` → `pavis/81_connection_pool_queue_behavior.sh`
- [ ] **3.1.4** `pavis/82_pool_high_limit.sh` → `pavis/82_connection_pool_high_limit.sh`
- [ ] **3.1.5** `pavis/83_pool_metric_tracking.sh` → `pavis/83_connection_pool_metric_tracking.sh`
- [ ] **3.1.6** `relay/30_fanout_multi.sh` → `relay/31_fanout_multi.sh`
- [ ] **3.1.7** `relay/31_fanout_late.sh` → `relay/32_fanout_late.sh`
- [ ] **3.1.8** `relay/50_transport_integrity.sh` → `relay/51_transport_integrity.sh`

---

### 3.2 Test Merges (3 merges)

**Tasks**:
- [ ] **3.2.1** Merge `relay/11_contract_republish.sh` + `relay/40_republish_stability.sh` → `relay/11_contract_republish_monotonicity.sh`
- [ ] **3.2.2** Delete `integrated/20_reload_switch.sh` (covered by pavis/20)
- [ ] **3.2.3** Simplify `pavis/51_resilience_retry.sh` to minimal smoke test

---

### 3.3 Add Missing Metrics Assertions (3 cases)

**Tasks**:
- [ ] **3.3.1** `pavis/44_routing_header_regex.sh` - Add `pavis_route_match_regex_input_too_large_total` assertion
- [ ] **3.3.2** `pavis/52_resilience_outlier_detection.sh` - Add `pavis_upstream_ejections_total` assertion
- [ ] **3.3.3** `pavis/32_lkg_relay_unavailable.sh` - Add `pavis_config_poll_errors_total` assertion

---

### 3.4 Add Missing Test Assertions (8 cases)

**Tasks**:
- [ ] **3.4.1** `pavis/50_resilience_timeout.sh` - Increase tolerance to <1000ms
- [ ] **3.4.2** `pavis/52_resilience_outlier_detection.sh` - Add re-admission test
- [ ] **3.4.3** `pavis/96_retry_body_buffer.sh` - Add strict mode test
- [ ] **3.4.4** `pavis/71_obs_access_log.sh` - Add assertions for all log fields
- [ ] **3.4.5** `pavis/90_operational_admin_api.sh` - Add /stats schema validation
- [ ] **3.4.6** `pavis/91_operational_graceful_shutdown.sh` - Reduce tolerance, add rejection test
- [ ] **3.4.7** `relay/40_concurrency_rapid.sh` - Add content verification
- [ ] **3.4.8** `relay/70_limits_oversize.sh` - Add max_pvs_bytes config test

---

### 3.5 Document SKIP/DEFER Tests (9 tests)

**Tasks**:
- [ ] **3.5.1** Update `docs/roadmap/roadmap.md` with re-enable criteria for TLS tests (7 cases)
- [ ] **3.5.2** Document `integrated/40_resilience_restart.sh` as redundant (covered by pavis/32)
- [ ] **3.5.3** Document `integrated/50_multiversion_chain.sh` waiting on relay monotonicity enforcement

---

## Appendix A: Quick Reference

### By Test File (Sorted by Priority)

#### Phase 0 (Critical)
1. **Runtime agent** (code fix required)
2. **34 test files** with sleep-based waits

#### Phase 1 (High)
1. `integrated/30_lkg_artifact.sh` - Clarify validation layering
2. `integrated/31_lkg_rejection.sh` - Fix env validation order
3. `pavis/33_semantic_validation_suite.sh` - Split layering violation
4. **12 test files** with unbounded retry loops

#### Phase 2 (Medium)
1. `pavis/35_atomicity_partial_corruption.sh` - NEW TEST
2. `relay/10_contract_opaque.sh` - Add negative test
3. 10 test files with missing assertions

#### Phase 3 (Low)
1. 8 file renames
2. 3 test merges/deletions
3. 11 minor assertion additions
4. Documentation updates

---

## Appendix B: Progress Tracking

**Phase 0: Critical Blockers**
- [ ] 0.1 Agent polling recovery (0/6 tasks)
- [ ] 0.2 Sleep-based waits (0/4 subtasks)

**Phase 1: High Priority**
- [ ] 1.1 Env validation before apply (0/6 tasks)
- [ ] 1.2 Validation layering (0/4 tasks)
- [ ] 1.3 Unbounded retry loops (0/2 tasks)
- [ ] 1.4 Layering violation split (0/7 tasks)

**Phase 2: Medium Priority**
- [ ] 2.1 Atomicity test (0/4 tasks)
- [ ] 2.2 Relay opacity test (0/3 tasks)
- [ ] 2.3 Missing assertions (0/10 tasks)

**Phase 3: Low Priority**
- [ ] 3.1 File renames (0/8 tasks)
- [ ] 3.2 Test merges (0/3 tasks)
- [ ] 3.3 Metrics assertions (0/3 tasks)
- [ ] 3.4 Test assertions (0/8 tasks)
- [ ] 3.5 Documentation (0/3 tasks)

**Overall Progress: 0/58 tasks complete (0%)**

---

## Appendix C: Dependencies

```
Phase 0 (no dependencies - can start immediately)
  ↓
Phase 1 (depends on Phase 0 helpers from 0.2.1)
  ↓
Phase 2 (can run parallel to Phase 1 after 0.2.1 complete)
  ↓
Phase 3 (can run parallel to Phase 2)
```

**Critical Path**: 0.1 → 0.2 → 1.x → Production Ready

---

## Appendix D: Validation Commands

After each phase, run full E2E suite:

```bash
# Full suite (all modes)
make e2e

# Individual suite validation
make e2e-pavis
make e2e-relay
make e2e-integrated

# Flake detection (run 3 times)
for i in 1 2 3; do make e2e; done

# Single test during development
make e2e-pavis CASE=30_lkg.sh
```

**Success Criteria**:
- Zero SKIP tests (except deferred features)
- Zero flakes across 3 runs
- All phases complete

---

**End of Remediation Plan**

**Next Steps**:
1. Review and approve this plan
2. Start Phase 0 (critical blockers)
3. Update progress checkboxes as tasks complete
4. Re-run audit after Phase 1 completion
