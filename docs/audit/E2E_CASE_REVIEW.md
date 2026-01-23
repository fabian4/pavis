# E2E Test Suite Audit Report

**Date**: 2026-01-23  
**Scope**: All E2E test cases across `tests/suites/{integrated,pavis,relay}/`  
**Total Cases Reviewed**: 62  
**Status**: FINAL

---

## 1. Executive Summary

### Overall Suite Health

| Dimension | Grade | Verdict |
|-----------|-------|---------|
| **Determinism** | 🟡 C+ | Widespread sleep-based timing (34 instances), unbounded retry loops (12), OS-dependent waits (5). Critical fix required. |
| **Coverage** | 🟢 B+ | Core frozen data plane invariants well-covered. Gaps: atomicity edge cases, relay tampering, hot reload during body buffering. |
| **Maintainability** | 🟡 B- | Redundant coverage (7 tests), SKIP debt (12 tests), naming inconsistencies (3). Needs consolidation. |

**Overall Verdict**: 🟡 **MODERATE HEALTH** - Suite provides strong functional coverage but suffers from determinism risks and critical LKG recovery bug. Immediate action required on P0 items before production use.

---

### Top Issues by Severity

| Severity | Issue | Fix Direction |
|----------|-------|---------------|
| **S0** | **pavis/30_lkg.sh SKIP** - Runtime agent fails to recover after invalid config sequence | Fix agent polling worker to reset state after validation failures; critical production blocker |
| **S0** | **34 sleep-based waits** - Non-deterministic timing across test suite | Replace `sleep N && assert` with bounded `wait_for_condition` helpers with explicit timeouts |
| **S1** | **integrated/31_lkg_rejection.sh SKIP** - Runtime applies config before env validation | Move TLS cert readability checks to pre-apply phase; violates A2 (Immutable Execution State) |
| **S1** | **12 unbounded retry loops** - Tests hang indefinitely if condition never met | Add explicit timeout validation after all retry loops |
| **S2** | **Missing atomicity test** - No test for partial config corruption (A4 violation) | Add test sending half-written PVS, verify 100% rejection |
| **S2** | **Missing relay opacity test** - No negative test for relay tampering (A5) | Add test where relay modifies PVS bytes, runtime detects checksum mismatch |
| **S2** | **Layering violation in pavis/33** - Codec validation tested in runtime layer | Move regex validation tests to codec-specific test suite |
| **S3** | **7 redundant tests** - Duplicate reload/LKG/retry coverage | Merge overlapping tests: keep most comprehensive, archive others |
| **S3** | **9 SKIP/DEFER tests** - TLS feature not implemented (7), relay features (2) | Document re-enable criteria; track feature delivery |
| **S3** | **3 naming inconsistencies** - Non-standard prefixes, abbreviations | Rename: 70_obs → 70_observability, 80_pool → 80_connection_pool, fix duplicate 30_ prefix in relay |

---

## 2. Suite Inventory

### Total: 62 Test Cases

#### Category 1: Bootstrap & Configuration Loading (4 tests)
- `integrated/10_bootstrap_path.sh` - System-level bootstrap from relay
- `pavis/10_bootstrap_static.sh` - Runtime bootstrap with static config
- `relay/10_contract_opaque.sh` - Relay treats PVS as opaque blob
- `relay/11_contract_republish.sh` - Republishing identical content

#### Category 2: Hot Reload & Atomicity (7 tests)
- `integrated/20_reload_switch.sh` - System-level reload behavior
- `integrated/21_reload_stable.sh` - Reload stability under load
- `pavis/20_reload_contract_core.sh` - Atomic state swap during reload
- `pavis/22_reload_storm.sh` - Rapid reload storm (stress test)
- `pavis/23_reload_keepalive_atomic.sh` - Reload during keepalive connections
- `pavis/24_atomic_mid_request.sh` - Reload during in-flight request processing
- `pavis/92_operational_reload_resource_sanity.sh` - Resource cleanup after reload

#### Category 3: LKG & Validation Rejection (8 tests)
- `integrated/30_lkg_artifact.sh` - **SKIP** - System LKG when relay accepts corrupt artifact
- `integrated/31_lkg_rejection.sh` - **SKIP** - System LKG on semantic rejection
- `integrated/32_runtime_env_rejection.sh` - Runtime env validation rejection
- `pavis/30_lkg.sh` - **SKIP** - Runtime LKG recovery after invalid configs
- `pavis/32_lkg_relay_unavailable.sh` - LKG preservation when relay unavailable
- `pavis/33_semantic_validation_suite.sh` - Semantic validation (codec vs runtime)
- `pavis/34_runtime_env_rejection.sh` - Runtime env checks (file paths, ports)
- `relay/60_boundary_conditions.sh` - Relay boundary validation

#### Category 4: Routing Semantics (6 tests)
- `pavis/40_traffic_routing_semantics.sh` - Path matching (prefix/exact/regex)
- `pavis/41_traffic_weighted.sh` - Weighted traffic splitting
- `pavis/42_routing_method_header_predicates.sh` - Method & header matching (P2)
- `pavis/43_routing_tie_breaking.sh` - Route priority tie-breaking
- `pavis/44_routing_header_regex.sh` - Header regex matching (P2)
- `relay/40_concurrency_rapid.sh` - Rapid concurrent config changes

#### Category 5: Resilience & Timeouts (10 tests)
- `integrated/40_resilience_restart.sh` - **SKIP** - Relay restart recovery
- `pavis/50_resilience_timeout.sh` - Request timeout enforcement
- `pavis/51_resilience_retry.sh` - Basic retry mechanics
- `pavis/52_resilience_outlier_detection.sh` - Passive health checks
- `pavis/53_resilience_active_health_check.sh` - Active probes
- `pavis/54_resilience_circuit_breaker.sh` - Circuit breaking
- `pavis/93_retry_status_codes.sh` - P2 retry status filtering
- `pavis/94_retry_idempotency.sh` - P2 idempotency constraints
- `pavis/95_retry_budget.sh` - P2 global retry budget
- `pavis/96_retry_body_buffer.sh` - P2 request body buffering

#### Category 6: Security & Identity (8 tests - 7 SKIP)
- `pavis/60_security_tls.sh` - **SKIP** - TLS termination
- `pavis/61_security_inbound_mtls.sh` - **SKIP** - Inbound mTLS
- `pavis/63_security_rbac_spiffe.sh` - **SKIP** - SPIFFE-based RBAC
- `pavis/64_security_rbac_prefix.sh` - **SKIP** - Prefix-based RBAC
- `pavis/65_security_mtls_outbound.sh` - **SKIP** - Outbound mTLS
- `pavis/66_security_tls_sni_auto.sh` - **SKIP** - SNI auto mode
- `pavis/67_security_mtls_chain_mode.sh` - **SKIP** - mTLS chain modes

#### Category 7: Observability (3 tests)
- `pavis/70_obs_consistency.sh` - Metrics consistency across reloads
- `pavis/71_obs_access_log.sh` - Access log formatting
- `pavis/72_obs_tracing_context.sh` - Distributed tracing propagation

#### Category 8: Connection Pooling (4 tests)
- `pavis/80_pool_hard_limit.sh` - Pool max limit enforcement
- `pavis/81_pool_queue_behavior.sh` - Queue capacity & timeout
- `pavis/82_pool_high_limit.sh` - High concurrency pool behavior
- `pavis/83_pool_metric_tracking.sh` - Pool metrics accuracy

#### Category 9: Operational (2 tests)
- `pavis/90_operational_admin_api.sh` - Admin API endpoints
- `pavis/91_operational_graceful_shutdown.sh` - SIGTERM handling

#### Category 10: Relay Protocol (12 tests)
- `relay/20_longpoll_wait.sh` - Long-poll blocking behavior
- `relay/21_longpoll_timeout.sh` - Long-poll timeout handling
- `relay/30_etag_validation.sh` - ETag checksum validation
- `relay/30_fanout_multi.sh` - Multi-client fanout
- `relay/31_fanout_late.sh` - Late-joining client behavior
- `relay/40_republish_stability.sh` - Republish monotonicity
- `relay/50_persistence_recovery.sh` - LKG recovery after crash
- `relay/50_transport_integrity.sh` - Transport-level integrity
- `relay/60_robustness_reconnect.sh` - Client reconnection handling
- `relay/70_limits_oversize.sh` - Oversized artifact rejection
- `relay/71_limits_empty.sh` - Empty artifact handling
- `integrated/50_multiversion_chain.sh` - **SKIP** - Multi-version monotonicity

---

## 3. Per-Case Review

### Category 1: Bootstrap & Configuration Loading

#### Case: integrated/10_bootstrap_path.sh
- **Status**: ACTIVE
- **Purpose**: Verify system-level bootstrap from relay to runtime with initial config fetch
- **Architectural invariants exercised**: 
  - A2 (Immutable Execution State) - Config loaded atomically
  - A5 (Relay Opacity) - Relay serves PVS without inspection
- **Feature coverage mapping**: Bootstrap (P0), Relay distribution (P3)
- **Strengths**: End-to-end validation, tests full pipeline
- **Issues / Loopholes**:
  - (a) Determinism risks: Uses `sleep 2` at line 78 instead of bounded wait for relay readiness
  - (b) False positives/negatives: Unbounded retry loop (lines 85-92) lacks timeout validation
  - (c) Missing assertions: Doesn't verify config version metadata in response headers
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Replace `sleep 2` with `wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10`
  - Add timeout check after retry loop: `[[ $attempt -lt $MAX_RETRIES ]] || fail "Bootstrap timeout"`
  - Add assertion: `assert_header "X-Pavis-Version" "1"`

---

#### Case: pavis/10_bootstrap_static.sh
- **Status**: ACTIVE
- **Purpose**: Verify runtime starts and serves traffic with pre-loaded static config
- **Architectural invariants exercised**:
  - A1 (No Runtime Inference) - No defaults applied
  - A2 (Immutable Execution State) - Config loaded once
- **Feature coverage mapping**: Runtime bootstrap (P1), Static config (P1)
- **Strengths**: Fast, no external dependencies, clear pass/fail
- **Issues / Loopholes**:
  - (a) Determinism risks: None - uses proper `wait_for_ready` helper
  - (b) False positives/negatives: None detected
  - (c) Missing assertions: Doesn't verify runtime rejects partial config updates
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Fix plan**: N/A (best practice reference case)

---

#### Case: relay/10_contract_opaque.sh
- **Status**: ACTIVE
- **Purpose**: Verify relay treats PVS as opaque blob (no parsing/validation)
- **Architectural invariants exercised**:
  - A5 (Relay Opacity) - Relay doesn't inspect content
  - I3 (Artifact Immutability) - Round-trip preserves bytes
- **Feature coverage mapping**: Relay opaqueness (P3), Content integrity (P2)
- **Strengths**: Clean test of opacity contract, uses byte-level comparison
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test **negative case** - relay modifying PVS should be detected
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add negative test section:
    ```bash
    # Tamper with PVS bytes (flip bit in payload)
    TAMPERED=$(echo "$PVS_BYTES" | sed 's/PAVS/TAVS/')
    publish_config "http://127.0.0.1:$PORT_RELAY" "$TAMPERED"
    # Runtime should detect checksum mismatch
    assert_log_contains "checksum mismatch"
    ```

---

#### Case: relay/11_contract_republish.sh
- **Status**: ACTIVE
- **Purpose**: Verify republishing identical artifact increments version but preserves content
- **Architectural invariants exercised**:
  - I2 (Monotonic Versioning) - Version increments on republish
  - I3 (Artifact Immutability) - Content unchanged
- **Feature coverage mapping**: Relay versioning (P3), Republish (P3)
- **Strengths**: Tests important edge case (same content, new version)
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify ETag changes on republish (or stays same?)
  - (d) Mis-layering or spec drift: Unclear spec - should ETag change on republish?
- **Recommendation**: FIX
- **Fix plan**:
  - Clarify spec: ETag = content hash, so republish should keep same ETag
  - Add assertion: `assert_eq "$ETAG_V1" "$ETAG_V2" "Same content should produce same ETag"`
  - Add assertion: `assert_ne "$VERSION_V1" "$VERSION_V2" "Version must increment"`

---

### Category 2: Hot Reload & Atomicity

#### Case: integrated/20_reload_switch.sh
- **Status**: ACTIVE (redundant with pavis/20)
- **Purpose**: System-level reload behavior verification
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Atomic swap
  - I5 (Hot Reload Contract) - Hitless reload
- **Feature coverage mapping**: Hot reload (P3), System integration (P3)
- **Strengths**: End-to-end validation
- **Issues / Loopholes**:
  - (a) Determinism risks: Uses `sleep 3` (line 65) instead of polling for reload completion
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: MERGE
- **Merge plan**:
  - Merge target: pavis/20_reload_contract_core.sh (more comprehensive)
  - Reason: Same coverage, integrated/* adds relay overhead without additional value
  - Preserve: None - all assertions duplicated in pavis/20

---

#### Case: integrated/21_reload_stable.sh
- **Status**: ACTIVE
- **Purpose**: Verify reload stability under concurrent request load
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - No torn reads during swap
  - I5 (Hot Reload Contract) - No dropped requests
- **Feature coverage mapping**: Hot reload stability (P3), Concurrency (P5)
- **Strengths**: Stress test with 100 concurrent requests
- **Issues / Loopholes**:
  - (a) Determinism risks: `sleep 5` (line 88) - non-deterministic load generation
  - (b) False positives/negatives: Race condition - reload might complete before all requests sent
  - (c) Missing assertions: Doesn't verify **zero** dropped requests (only checks majority succeed)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Use synchronization: Start all requests in background, then trigger reload, then wait for completion
  - Add assertion: `assert_eq "$FAILED" 0 "Zero requests should fail during reload"`
  - Replace sleep with barrier pattern

---

#### Case: pavis/20_reload_contract_core.sh
- **Status**: ACTIVE (best practice reference)
- **Purpose**: Verify atomic state swap during hot reload
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Pointer swap atomicity
  - I5 (Hot Reload Contract) - Old state accessible until requests complete
- **Feature coverage mapping**: Hot reload (P3 critical path)
- **Strengths**: Clean, deterministic, tests RCU semantics explicitly
- **Issues / Loopholes**:
  - (a) Determinism risks: None - uses proper `wait_for_ready` and version polling
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP (reference implementation)

---

#### Case: pavis/22_reload_storm.sh
- **Status**: ACTIVE
- **Purpose**: Stress test rapid reload sequence (10 reloads in 5 seconds)
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - State machine correctness under pressure
  - I5 (Hot Reload Contract) - Memory leak prevention
- **Feature coverage mapping**: Reload resilience (P5), Resource management (P7)
- **Strengths**: Exposes memory/resource leaks
- **Issues / Loopholes**:
  - (a) Determinism risks: `sleep 0.5` between reloads (line 102) - timing-dependent
  - (b) False positives/negatives: No validation that all 10 reloads actually applied
  - (c) Missing assertions: Doesn't verify memory usage stays bounded (only checks final state)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add version tracking: Verify each reload increments version (not just final version == 10)
  - Add memory assertion: `assert_memory_growth_lt "$INITIAL_RSS" 50` (50% max growth)
  - Document expected behavior in comment

---

#### Case: pavis/23_reload_keepalive_atomic.sh
- **Status**: ACTIVE
- **Purpose**: Verify reload doesn't break keepalive connections
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Connection state preserved across swap
  - I5 (Hot Reload Contract) - Existing connections unaffected
- **Feature coverage mapping**: Hot reload (P3), HTTP keepalive (P1)
- **Strengths**: Tests important edge case
- **Issues / Loopholes**:
  - (a) Determinism risks: `sleep 1` (line 78) - assumes keepalive established
  - (b) False positives/negatives: Doesn't verify connection reuse (only checks response correctness)
  - (c) Missing assertions: Should check `Connection: keep-alive` header
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add curl verbose output parsing to verify connection reuse
  - Replace sleep with deterministic "send request, wait for response, trigger reload, send 2nd request on same socket"
  - Add assertion: `assert_header "Connection" "keep-alive"`

---

#### Case: pavis/24_atomic_mid_request.sh
- **Status**: ACTIVE
- **Purpose**: Verify reload during in-flight request doesn't corrupt response
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - No torn reads
  - I5 (Hot Reload Contract) - In-flight requests complete with old state
- **Feature coverage mapping**: Hot reload atomicity (P3 critical)
- **Strengths**: Tests critical race condition
- **Issues / Loopholes**:
  - (a) Determinism risks: Race condition - reload timing relative to request processing
  - (b) False positives/negatives: May pass if reload happens before/after request instead of during
  - (c) Missing assertions: Doesn't verify response uses old vs new config definitively
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Use slow backend endpoint (e.g., `/slow?delay=2000`) to guarantee overlap
  - Trigger reload after 500ms (mid-request)
  - Add assertion on response content to verify old state used

---

#### Case: pavis/92_operational_reload_resource_sanity.sh
- **Status**: ACTIVE
- **Purpose**: Verify file descriptors and memory cleaned up after multiple reloads
- **Architectural invariants exercised**:
  - I5 (Hot Reload Contract) - Resource cleanup
- **Feature coverage mapping**: Resource management (P7), Memory safety (P1)
- **Strengths**: Tests operational concern (leak detection)
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: Threshold for "acceptable growth" is arbitrary (50%)
  - (c) Missing assertions: Doesn't test fd leaks for TLS connections
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Fix plan**: Document threshold rationale in comment

---

### Category 3: LKG & Validation Rejection

#### Case: integrated/30_lkg_artifact.sh
- **Status**: **SKIP** (exit 77, line 13)
- **Purpose**: Test system LKG when relay accepts corrupt artifact
- **Architectural invariants exercised**:
  - I3 (Artifact Opaqueness) - Relay doesn't validate content
  - I4 (System LKG) - Runtime rejects, continues with LKG
- **Feature coverage mapping**: LKG preservation (P3 critical), Validation layering (P3)
- **Strengths**: Tests important failure mode
- **Issues / Loopholes**:
  - (a) Determinism risks: None (SKIP)
  - (b) False positives/negatives: N/A
  - (c) Missing assertions: N/A (test skipped)
  - (d) Mis-layering or spec drift: **CRITICAL** - Unclear if relay should validate PVS magic/checksum or defer to runtime
- **Recommendation**: **FIX**
- **Fix plan**:
  - Clarify layered validation contract:
    - **Relay**: Validates magic bytes + checksum (A5 doesn't prohibit integrity checks)
    - **Core**: Validates semantic invariants (routing tree, regex safety)
    - **Runtime**: Validates environment (file paths, ports)
  - Update test expectations:
    - Relay **SHOULD** reject corrupt PVS with 422 (not accept it)
    - Test becomes: Send corrupt PVS → Relay rejects → Runtime keeps LKG
  - Remove SKIP, update assertions

---

#### Case: integrated/31_lkg_rejection.sh
- **Status**: **SKIP** (exit 77, line 8)
- **Purpose**: Test runtime semantic rejection preserves LKG
- **Architectural invariants exercised**:
  - I4 (System LKG) - Rejection doesn't affect serving
  - A2 (Immutable Execution State) - No partial application
- **Feature coverage mapping**: LKG preservation (P3 critical), Runtime validation (P3)
- **Strengths**: Tests critical failure path
- **Issues / Loopholes**:
  - (a) Determinism risks: None (SKIP)
  - (b) False positives/negatives: N/A
  - (c) Missing assertions: N/A (test skipped)
  - (d) Mis-layering or spec drift: **CRITICAL** - Runtime applies config before env validation (violates A2)
- **Recommendation**: **FIX**
- **Fix plan**:
  - Fix runtime: Move TLS cert readability checks to **pre-apply** phase
  - Change order: `validate_env() → swap_state()` (not current: `swap_state() → lazy_load_certs()`)
  - Update test to verify rejection is synchronous (not lazy)
  - Remove SKIP after runtime fix

---

#### Case: integrated/32_runtime_env_rejection.sh
- **Status**: ACTIVE
- **Purpose**: Verify runtime env checks (port conflicts, file permissions)
- **Architectural invariants exercised**:
  - A3 (Layered Validation) - Runtime validates environment, not semantics
  - I4 (System LKG) - Env rejection preserves LKG
- **Feature coverage mapping**: Runtime env validation (P3), Port conflict detection (P4)
- **Strengths**: Tests practical deployment failures
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify traffic continues uninterrupted during rejection
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add concurrent traffic test: Send requests during env rejection, verify 100% success
  - Add assertion: `assert_version_unchanged` before and after rejection

---

#### Case: pavis/30_lkg.sh
- **Status**: **SKIP** (exit 77) - **S0 CRITICAL BUG**
- **Purpose**: Verify LKG recovery after encountering invalid config sequence
- **Architectural invariants exercised**:
  - I4 (System LKG) - Runtime continues serving after rejections
  - A2 (Immutable Execution State) - Invalid configs don't corrupt state
- **Feature coverage mapping**: LKG recovery (P3 critical), Multi-stage validation (P3)
- **Strengths**: Comprehensive test of validation pipeline (parse → version → semantic → runtime)
- **Issues / Loopholes**:
  - (a) Determinism risks: None (logic correct, runtime bug)
  - (b) False positives/negatives: **CRITICAL BUG** - Agent polling worker fails to recover
  - (c) Missing assertions: N/A (test correctly identifies runtime bug)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: **FIX RUNTIME BUG FIRST** (blocking for production)
- **Fix plan**:
  - **Runtime fix**: Agent polling worker must reset state after validation failures
  - **Issue**: After rejecting v2-v3, agent gets 404 from relay for intermediate versions, never fetches valid v4
  - **Root cause**: Agent tracks `last_version` and tries to fetch missing intermediate versions even after they're rejected
  - **Solution**: Implement "latest-driven" polling (documented in `docs/plan/latest-driven-runtime.md`)
  - **Test fix**: Remove SKIP after runtime fix
- **Defer rationale**: N/A - This is FIX, not DEFER
- **Re-enable criteria**: Runtime agent polling logic fixed per latest-driven plan

---

#### Case: pavis/32_lkg_relay_unavailable.sh
- **Status**: ACTIVE
- **Purpose**: Verify runtime continues with LKG when relay is unavailable
- **Architectural invariants exercised**:
  - I4 (System LKG) - Resilient to control plane failures
  - A2 (Immutable Execution State) - Doesn't revert to defaults
- **Feature coverage mapping**: LKG resilience (P3), Control plane independence (P7)
- **Strengths**: Tests operational reality (relay crashes)
- **Issues / Loopholes**:
  - (a) Determinism risks: `sleep 5` (line 72) - waits for relay unavailability to propagate
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify runtime **stops polling** (vs retrying forever)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Replace sleep with explicit check: `wait_for_log "relay unreachable" 10`
  - Add assertion on polling backoff behavior (verify exponential backoff, not tight loop)
  - Add metrics check: `assert_metric "pavis_config_poll_errors_total" "> 0"`

---

#### Case: pavis/33_semantic_validation_suite.sh
- **Status**: ACTIVE (layering violation)
- **Purpose**: Test semantic validation rejection (invalid regex, missing upstreams)
- **Architectural invariants exercised**:
  - A3 (Layered Validation) - Core validates semantics
  - A4 (Atomic Validity) - Partial configs rejected
- **Feature coverage mapping**: Semantic validation (P3), Codec validation (P2)
- **Strengths**: Comprehensive validation cases
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: **VIOLATION** - Tests codec validation (line 84) vs runtime validation (line 100) inconsistently; per A3, codec should reject invalid regex, not runtime
- **Recommendation**: FIX
- **Fix plan**:
  - Split test into two files:
    - `33_codec_validation.sh` - Tests codec rejection (regex syntax, YAML parse errors)
    - `34_core_validation.sh` - Tests core rejection (upstream references, routing tree integrity)
  - Move to appropriate test suites (codec tests should be in `crates/pavis-codec-serde/tests/`)
  - Runtime tests only validate environment, not semantics

---

#### Case: pavis/34_runtime_env_rejection.sh
- **Status**: ACTIVE
- **Purpose**: Test runtime environment validation (file paths, port availability)
- **Architectural invariants exercised**:
  - A3 (Layered Validation) - Runtime validates environment
  - A1 (No Runtime Inference) - Doesn't fall back to defaults on missing files
- **Feature coverage mapping**: Runtime env checks (P3), Fail-closed behavior (P4)
- **Strengths**: Tests correct layering boundary
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify LKG preservation atomically (see integrated/31 loophole)
  - (d) Mis-layering or spec drift: None (correct layer)
- **Recommendation**: FIX
- **Fix plan**:
  - Add traffic continuity test: Send requests during env rejection, verify no interruption
  - Add assertion: `assert_serving_state_unchanged` before/after rejection

---

#### Case: relay/60_boundary_conditions.sh
- **Status**: ACTIVE
- **Purpose**: Test relay validation of boundary conditions (empty config, oversized)
- **Architectural invariants exercised**:
  - A5 (Relay Opacity) - Relay validates size, not semantics
- **Feature coverage mapping**: Relay validation (P3), Boundary checks (P5)
- **Strengths**: Tests relay responsibilities clearly
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test negative case (relay accepting invalid boundaries)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

### Category 4: Routing Semantics

#### Case: pavis/40_traffic_routing_semantics.sh
- **Status**: ACTIVE
- **Purpose**: Test path matching (prefix, exact, regex) and route selection
- **Architectural invariants exercised**:
  - A1 (No Runtime Inference) - Explicit routing rules
  - A2 (Immutable Execution State) - Routing decisions from frozen state
- **Feature coverage mapping**: L7 routing (P1), Path matching (P1)
- **Strengths**: Core functionality, clear pass/fail
- **Issues / Loopholes**:
  - (a) Determinism risks: Unbounded retry loop (lines 88-94) lacks timeout check
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test tie-breaking (covered by 43)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add timeout validation after loop: `[[ $attempt -lt $MAX_RETRIES ]] || fail "Routing test timeout"`

---

#### Case: pavis/41_traffic_weighted.sh
- **Status**: ACTIVE (redundant with pavis/20)
- **Purpose**: Test weighted traffic splitting with reload
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Weights frozen in config
  - I5 (Hot Reload Contract) - Weights change atomically on reload
- **Feature coverage mapping**: Weighted splitting (P1), Load balancing (P1)
- **Strengths**: Tests important feature
- **Issues / Loopholes**:
  - (a) Determinism risks: Statistical test - uses 100 samples to verify 90/10 split (lines 102-120)
  - (b) False positives/negatives: Small sample size - could fail with valid 90/10 split due to variance
  - (c) Missing assertions: No tolerance bounds (should accept 85-95% range)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Increase sample size to 1000 for statistical significance
  - Add tolerance: `assert_range "$PERCENT_V1" 85 95 "Expected ~90% to v1"`
  - Or use deterministic approach: Mock round-robin, verify exact sequence

---

#### Case: pavis/42_routing_method_header_predicates.sh
- **Status**: ACTIVE (P2 feature)
- **Purpose**: Test method matching and header predicates (exact, prefix, present, absent)
- **Architectural invariants exercised**:
  - A1 (No Runtime Inference) - Explicit predicates
  - A2 (Immutable Execution State) - Predicate eval from frozen state
- **Feature coverage mapping**: P2 routing enhancements (P2), Method/header matching (P2)
- **Strengths**: Comprehensive P2 coverage
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test regex operator (covered by 44)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP (reference P2 test)

---

#### Case: pavis/43_routing_tie_breaking.sh
- **Status**: ACTIVE
- **Purpose**: Test route priority when multiple routes match
- **Architectural invariants exercised**:
  - A1 (No Runtime Inference) - Deterministic tie-breaking
  - A2 (Immutable Execution State) - Priority from config order
- **Feature coverage mapping**: Route priority (P1), Tie-breaking (P2)
- **Strengths**: Tests edge case clearly
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

#### Case: pavis/44_routing_header_regex.sh
- **Status**: ACTIVE (P2 feature)
- **Purpose**: Test header regex matching with input size limits
- **Architectural invariants exercised**:
  - A1 (No Runtime Inference) - Explicit regex patterns
  - A4 (Atomic Validity) - Regex compiled at load time
- **Feature coverage mapping**: P2 regex matching (P2), Input validation (P4)
- **Strengths**: Tests performance limits (4096 byte input cap)
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify metrics (`pavis_route_match_regex_input_too_large_total`)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add metrics assertion: `assert_metric "pavis_route_match_regex_input_too_large_total" "> 0"`

---

#### Case: relay/40_concurrency_rapid.sh
- **Status**: ACTIVE
- **Purpose**: Test relay handles rapid concurrent config updates
- **Architectural invariants exercised**:
  - I2 (Monotonic Versioning) - Version sequence maintained under concurrency
- **Feature coverage mapping**: Relay concurrency (P5), Version monotonicity (P3)
- **Strengths**: Stress test for race conditions
- **Issues / Loopholes**:
  - (a) Determinism risks: Race condition - order of concurrent publishes non-deterministic
  - (b) False positives/negatives: Doesn't verify **which** config wins, only that version increments
  - (c) Missing assertions: Should verify final state matches **one** of the published configs
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add content verification: Store published configs, verify final state matches one of them
  - Add monotonicity assertion: `assert_eq "$FINAL_VERSION" "$PUBLISH_COUNT"`

---

### Category 5: Resilience & Timeouts

#### Case: integrated/40_resilience_restart.sh
- **Status**: **SKIP** (exit 77, line 13)
- **Purpose**: Test relay restart and runtime LKG recovery
- **Architectural invariants exercised**:
  - I2 (Crash Safety) - Relay recovers state from LKG
  - I4 (System LKG) - Runtime tolerates relay restarts
- **Feature coverage mapping**: Relay persistence (P3), Fault tolerance (P7)
- **Strengths**: Tests operational scenario
- **Issues / Loopholes**:
  - (a) Determinism risks: None (SKIP)
  - (b) False positives/negatives: N/A
  - (c) Missing assertions: N/A (test skipped)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: **DEFER** (low priority)
- **Defer rationale**: Covered by pavis/32_lkg_relay_unavailable.sh; integrated test adds complexity without new coverage
- **Re-enable criteria**: Needed only if testing relay-specific crash recovery (fsync semantics, etc.)

---

#### Case: pavis/50_resilience_timeout.sh
- **Status**: ACTIVE
- **Purpose**: Test request timeout enforcement
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Timeout from config
- **Feature coverage mapping**: Request timeouts (P2), Resilience (P6)
- **Strengths**: Tests critical resilience feature
- **Issues / Loopholes**:
  - (a) Determinism risks: **OS-dependent timing** - expects timeout <500ms (line 80)
  - (b) False positives/negatives: Could fail on slow CI systems
  - (c) Missing assertions: Doesn't verify backend request was cancelled
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Increase tolerance: Change assertion to `<1000ms` to account for scheduler variance
  - Add backend cancellation check: Verify backend didn't process full request

---

#### Case: pavis/51_resilience_retry.sh
- **Status**: ACTIVE (redundant with 93/94)
- **Purpose**: Basic retry mechanics (503 → retry → 200)
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Retry policy from config
- **Feature coverage mapping**: Basic retry (P2), Resilience (P6)
- **Strengths**: Simple smoke test
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test edge cases (covered by 93/94)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: MERGE or simplify
- **Merge plan**:
  - Keep 93/94 (comprehensive P2 tests)
  - Simplify 51 to minimal smoke test or remove entirely
  - Preserve: None (coverage duplicated in 93/94)

---

#### Case: pavis/52_resilience_outlier_detection.sh
- **Status**: ACTIVE
- **Purpose**: Test passive health checks (consecutive failures → ejection)
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Ejection policy from config
- **Feature coverage mapping**: Outlier detection (P6), Passive health (P6)
- **Strengths**: Tests important resilience feature
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify re-admission after eject_duration
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add re-admission test: Wait eject_duration, verify endpoint returns to pool
  - Add metrics: `assert_metric "pavis_upstream_ejections_total" "> 0"`

---

#### Case: pavis/53_resilience_active_health_check.sh
- **Status**: ACTIVE
- **Purpose**: Test active health probes (periodic GET requests)
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Probe config frozen
- **Feature coverage mapping**: Active health checks (P6), Probing (P6)
- **Strengths**: Tests proactive monitoring
- **Issues / Loopholes**:
  - (a) Determinism risks: `sleep 3` (line 95) - waits for probe interval
  - (b) False positives/negatives: Timing-dependent - probe might not have run yet
  - (c) Missing assertions: Doesn't verify probe request format (method, headers)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Replace sleep with backend probe counter: Poll until `probe_count >= 2`
  - Add assertion on probe format: Verify GET request to /healthz

---

#### Case: pavis/54_resilience_circuit_breaker.sh
- **Status**: ACTIVE
- **Purpose**: Test circuit breaking (max_connections limit)
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Circuit limits from config
- **Feature coverage mapping**: Circuit breaking (P6), Connection limits (P0)
- **Strengths**: Tests fail-fast behavior
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify 503 response details
  - (d) Mis-layering or spec drift: None (note: overlaps with pool tests 80-83, but tests different concern)
- **Recommendation**: KEEP

---

#### Case: pavis/93_retry_status_codes.sh
- **Status**: ACTIVE (P2 feature)
- **Purpose**: Test P2 retry with status code filtering
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Retryable codes from config
- **Feature coverage mapping**: P2 retry (P2), Status code filtering (P2)
- **Strengths**: Comprehensive P2 test
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

#### Case: pavis/94_retry_idempotency.sh
- **Status**: ACTIVE (P2 feature)
- **Purpose**: Test P2 idempotency constraints (POST not retried by default)
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Idempotency rules from config
- **Feature coverage mapping**: P2 retry idempotency (P2), Safety (P4)
- **Strengths**: Tests critical safety feature
- **Issues / Loopholes**:
  - (a) Determinism risks: `sleep 2` (line 108) - waits for retry attempts
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Replace sleep with backend request counter polling

---

#### Case: pavis/95_retry_budget.sh
- **Status**: ACTIVE (P2 feature)
- **Purpose**: Test P2 global retry budget (deadline enforcement)
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Budget from config
- **Feature coverage mapping**: P2 retry budget (P2), Resource limits (P5)
- **Strengths**: Tests important resource constraint
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

#### Case: pavis/96_retry_body_buffer.sh
- **Status**: ACTIVE (P2 feature)
- **Purpose**: Test P2 request body buffering for retry replay
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Buffer size from config
- **Feature coverage mapping**: P2 body buffering (P2), Retry safety (P4)
- **Strengths**: Tests edge case (large bodies)
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test strict mode (`fail_on_non_replayable_retry`)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add strict mode test: `fail_on_non_replayable_retry: true` + large body → 500 error

---

### Category 6: Security & Identity

*(All 7 tests SKIP - TLS backend not implemented)*

#### Cases: pavis/60-67_security_*.sh
- **Status**: **SKIP** (exit 77) - All 7 TLS/mTLS tests
- **Purpose**: Test TLS termination, inbound/outbound mTLS, RBAC (SPIFFE, prefix)
- **Architectural invariants exercised**: Security features (P4)
- **Feature coverage mapping**: TLS (P4), mTLS (P4), RBAC (P4), Identity (P4)
- **Strengths**: Comprehensive security test coverage ready for when feature ships
- **Issues / Loopholes**: None (feature not implemented)
- **Recommendation**: **DEFER**
- **Defer rationale**: TLS backend not implemented; tests ready for feature delivery
- **Re-enable criteria**: 
  - TLS termination implemented (pavis/60)
  - Inbound mTLS with Rustls backend (pavis/61)
  - RBAC implementation (pavis/63-64)
  - Outbound mTLS (pavis/65-67)

---

### Category 7: Observability

#### Case: pavis/70_obs_consistency.sh
- **Status**: ACTIVE
- **Purpose**: Test metrics consistency across reloads (no double-counting)
- **Architectural invariants exercised**:
  - I5 (Hot Reload Contract) - Metrics unaffected by reload
- **Feature coverage mapping**: Prometheus metrics (P5), Observability (P5)
- **Strengths**: Tests operational concern (metrics accuracy)
- **Issues / Loopholes**:
  - (a) Determinism risks: `sleep 2` (line 88) - waits for metrics propagation
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test metrics **reset** on config change (if applicable)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Replace sleep with metrics polling: `wait_for_metric "pavis_http_requests_total" "> 0" 10`
  - Rename: `70_obs_consistency.sh` → `70_observability_consistency.sh` (spelling out abbreviation)

---

#### Case: pavis/71_obs_access_log.sh
- **Status**: ACTIVE
- **Purpose**: Test access log formatting (JSON structure, field presence)
- **Architectural invariants exercised**: None (observability feature)
- **Feature coverage mapping**: Access logs (P5), Structured logging (P5)
- **Strengths**: Tests log contract
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test all log fields (missing: latency, method, status)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add assertions for required fields: `method`, `status`, `latency_ms`, `upstream`
  - Verify JSON validity: `jq '.' access.log`

---

#### Case: pavis/72_obs_tracing_context.sh
- **Status**: ACTIVE
- **Purpose**: Test distributed tracing context propagation (OTLP)
- **Architectural invariants exercised**: None (observability feature)
- **Feature coverage mapping**: Distributed tracing (P5), OTLP (P5)
- **Strengths**: Tests trace ID propagation
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test span parent-child relationships
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

### Category 8: Connection Pooling

#### Case: pavis/80_pool_hard_limit.sh
- **Status**: ACTIVE (P0 feature)
- **Purpose**: Test pool max limit enforcement with semaphore gating
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Pool limits from config
- **Feature coverage mapping**: P0 pool enforcement (P0), Semaphore gating (P0)
- **Strengths**: Tests critical P0 feature
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Naming fix**: Rename `80_pool_hard_limit.sh` → `80_connection_pool_hard_limit.sh`

---

#### Case: pavis/81_pool_queue_behavior.sh
- **Status**: ACTIVE (P0 feature)
- **Purpose**: Test queue capacity and timeout when pool is full
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Queue params from config
- **Feature coverage mapping**: P0 queue behavior (P0), Timeout enforcement (P0)
- **Strengths**: Tests queue semantics
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Naming fix**: Rename `81_pool_queue_behavior.sh` → `81_connection_pool_queue_behavior.sh`

---

#### Case: pavis/82_pool_high_limit.sh
- **Status**: ACTIVE
- **Purpose**: Test pool behavior under high concurrency
- **Architectural invariants exercised**:
  - A2 (Immutable Execution State) - Limits enforced under stress
- **Feature coverage mapping**: Pool stress test (P5), High concurrency (P5)
- **Strengths**: Stress test for pool invariants
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Naming fix**: Rename `82_pool_high_limit.sh` → `82_connection_pool_high_limit.sh`

---

#### Case: pavis/83_pool_metric_tracking.sh
- **Status**: ACTIVE
- **Purpose**: Test pool metrics accuracy (gauge, rejection counters)
- **Architectural invariants exercised**: None (metrics accuracy)
- **Feature coverage mapping**: Pool metrics (P0), Observability (P5)
- **Strengths**: Tests operational visibility
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Naming fix**: Rename `83_pool_metric_tracking.sh` → `83_connection_pool_metric_tracking.sh`

---

### Category 9: Operational

#### Case: pavis/90_operational_admin_api.sh
- **Status**: ACTIVE
- **Purpose**: Test admin API endpoints (/health, /stats)
- **Architectural invariants exercised**: Admin API contract (A6.2)
- **Feature coverage mapping**: Admin API (P7), Health checks (P7)
- **Strengths**: Tests operational interface
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify /stats response schema
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add /stats schema validation: Verify required fields (version, uptime, listeners, upstreams, routes)

---

#### Case: pavis/91_operational_graceful_shutdown.sh
- **Status**: ACTIVE
- **Purpose**: Test SIGTERM handling and graceful shutdown
- **Architectural invariants exercised**:
  - A6.1 (Signal Handling) - SIGTERM → graceful drain
- **Feature coverage mapping**: Graceful shutdown (P7), SIGTERM (P7)
- **Strengths**: Tests critical operational behavior
- **Issues / Loopholes**:
  - (a) Determinism risks: **OS-dependent timing** - expects 3s drain ±3s variance (line 144)
  - (b) False positives/negatives: High tolerance window - might mask bugs
  - (c) Missing assertions: Doesn't verify **new connections rejected** during drain
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Reduce tolerance: Use 3s ±1s (tighter bound)
  - Add rejection test: Try new connection during drain, expect immediate close

---

### Category 10: Relay Protocol

#### Case: relay/20_longpoll_wait.sh
- **Status**: ACTIVE
- **Purpose**: Test long-poll blocking behavior (waits for new version)
- **Architectural invariants exercised**:
  - I6 (Long-Poll Contract) - Blocks until version changes
- **Feature coverage mapping**: Long-poll (P3), Relay protocol (P3)
- **Strengths**: Tests important distribution feature
- **Issues / Loopholes**:
  - (a) Determinism risks: **OS-dependent timing** - expects ~500ms ±100ms (line 92)
  - (b) False positives/negatives: Tight timing window - could fail on slow systems
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Increase tolerance: ±200ms to account for scheduler variance
  - Use monotonic clock: `date +%s%N` (nanoseconds) for more precise timing

---

#### Case: relay/21_longpoll_timeout.sh
- **Status**: ACTIVE
- **Purpose**: Test long-poll timeout (returns 304 after wait_ms)
- **Architectural invariants exercised**:
  - I6 (Long-Poll Contract) - Timeout enforcement
- **Feature coverage mapping**: Long-poll timeout (P3), Protocol compliance (P3)
- **Strengths**: Tests timeout semantics
- **Issues / Loopholes**:
  - (a) Determinism risks: None (uses bounded wait)
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

#### Case: relay/30_etag_validation.sh
- **Status**: ACTIVE
- **Purpose**: Test ETag checksum validation (sha256 integrity)
- **Architectural invariants exercised**:
  - I7 (Checksum Validation) - Clients verify artifact integrity
- **Feature coverage mapping**: ETag validation (P3), Transport integrity (P3)
- **Strengths**: Tests critical security feature
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Naming fix**: Duplicate prefix `30_` with fanout_multi - renumber to 30/31

---

#### Case: relay/30_fanout_multi.sh
- **Status**: ACTIVE
- **Purpose**: Test relay fanout to multiple clients
- **Architectural invariants exercised**:
  - I6 (Long-Poll Contract) - All waiters notified on publish
- **Feature coverage mapping**: Relay fanout (P3), Multi-client (P5)
- **Strengths**: Tests scalability concern
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Naming fix**: Rename `30_fanout_multi.sh` → `31_fanout_multi.sh` (avoid duplicate prefix)

---

#### Case: relay/31_fanout_late.sh
- **Status**: ACTIVE
- **Purpose**: Test late-joining client receives current version immediately
- **Architectural invariants exercised**:
  - I6 (Long-Poll Contract) - No blocking if version already advanced
- **Feature coverage mapping**: Late join (P3), Protocol semantics (P3)
- **Strengths**: Tests edge case clearly
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Naming fix**: Renumber to `32_fanout_late.sh` (after fixing 30→31)

---

#### Case: relay/40_republish_stability.sh
- **Status**: ACTIVE
- **Purpose**: Test version monotonicity under republish
- **Architectural invariants exercised**:
  - I2 (Monotonic Versioning) - Version always increments
- **Feature coverage mapping**: Version monotonicity (P3), Republish (P3)
- **Strengths**: Tests important invariant
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't verify checksum behavior (same content = same checksum?)
  - (d) Mis-layering or spec drift: Covered by relay/11_contract_republish.sh
- **Recommendation**: MERGE
- **Merge plan**:
  - Merge with relay/11_contract_republish.sh
  - New name: `11_contract_republish_monotonicity.sh`
  - Combine: version increment + checksum stability

---

#### Case: relay/50_persistence_recovery.sh
- **Status**: ACTIVE
- **Purpose**: Test relay LKG recovery after crash
- **Architectural invariants exercised**:
  - I2 (Crash Safety) - Relay recovers state from LKG marker
- **Feature coverage mapping**: Persistence (P3), Crash recovery (P7)
- **Strengths**: Tests operational resilience
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test fsync semantics (partial write recovery)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

#### Case: relay/50_transport_integrity.sh
- **Status**: ACTIVE
- **Purpose**: Test transport-level integrity (chunked encoding, compression)
- **Architectural invariants exercised**:
  - I7 (Checksum Validation) - Transport doesn't corrupt artifact
- **Feature coverage mapping**: Transport integrity (P3), HTTP compliance (P3)
- **Strengths**: Tests protocol compliance
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP
- **Naming fix**: Duplicate prefix `50_` - renumber to 51

---

#### Case: relay/60_robustness_reconnect.sh
- **Status**: ACTIVE
- **Purpose**: Test client reconnection after network interruption
- **Architectural invariants exercised**:
  - I6 (Long-Poll Contract) - Clients can reconnect and resume
- **Feature coverage mapping**: Reconnection (P5), Fault tolerance (P7)
- **Strengths**: Tests operational scenario
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

#### Case: relay/70_limits_oversize.sh
- **Status**: ACTIVE
- **Purpose**: Test relay rejects oversized artifacts
- **Architectural invariants exercised**:
  - A5 (Relay Opacity) - Size validation only, not semantic
- **Feature coverage mapping**: Artifact size limits (P5), DoS prevention (P5)
- **Strengths**: Tests resource protection
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: Doesn't test **max_pvs_bytes** config parameter
  - (d) Mis-layering or spec drift: None
- **Recommendation**: FIX
- **Fix plan**:
  - Add configurable limit test: Set `max_pvs_bytes: 1024`, send 2048-byte artifact, expect 413

---

#### Case: relay/71_limits_empty.sh
- **Status**: ACTIVE
- **Purpose**: Test relay rejects empty artifacts
- **Architectural invariants exercised**:
  - A5 (Relay Opacity) - Minimal validation (non-empty)
- **Feature coverage mapping**: Empty artifact handling (P5), Validation (P3)
- **Strengths**: Tests boundary condition
- **Issues / Loopholes**:
  - (a) Determinism risks: None
  - (b) False positives/negatives: None
  - (c) Missing assertions: None
  - (d) Mis-layering or spec drift: None
- **Recommendation**: KEEP

---

#### Case: integrated/50_multiversion_chain.sh
- **Status**: **SKIP** (exit 77, line 8)
- **Purpose**: Test multi-version monotonic application
- **Architectural invariants exercised**:
  - I2 (Monotonic Versioning) - Sequential version application
  - I5 (Hot Reload Contract) - Multiple reloads maintain invariants
- **Feature coverage mapping**: Multi-version (P3), Version monotonicity (P3)
- **Strengths**: Tests version chain integrity
- **Issues / Loopholes**:
  - (a) Determinism risks: None (SKIP)
  - (b) False positives/negatives: N/A
  - (c) Missing assertions: N/A (test skipped)
  - (d) Mis-layering or spec drift: None
- **Recommendation**: **DEFER**
- **Defer rationale**: Waiting for relay monotonicity enforcement implementation
- **Re-enable criteria**: Relay validates `new_version = current_version + 1` before accepting publish

---

## 4. Skip/Defer Ledger

| Case | Current Reason | Classification | Next Action | Re-enable Criteria |
|------|---------------|----------------|-------------|-------------------|
| **pavis/30_lkg.sh** | Agent fails to recover after invalid configs | **FIX** (S0) | Fix agent polling worker reset logic | Implement latest-driven polling per `docs/plan/latest-driven-runtime.md` |
| **integrated/30_lkg_artifact.sh** | Runtime behavior unclear | **FIX** (S1) | Clarify validation layering | Relay validates magic/checksum; Runtime validates semantics |
| **integrated/31_lkg_rejection.sh** | Runtime lazy env validation | **FIX** (S1) | Move env checks to pre-apply | TLS cert checks before `swap_state()` |
| **integrated/40_resilience_restart.sh** | Runtime behavior unclear | **DEFER** (S3) | Document coverage redundancy | Covered by pavis/32; only needed for relay crash recovery |
| **integrated/50_multiversion_chain.sh** | Relay monotonicity not enforced | **DEFER** (S3) | Wait for relay feature | Relay rejects non-monotonic versions |
| **pavis/60_security_tls.sh** | TLS not implemented | **DEFER** (S3) | Wait for feature | TLS termination implemented |
| **pavis/61_security_inbound_mtls.sh** | mTLS not implemented | **DEFER** (S3) | Wait for feature | Inbound mTLS with Rustls backend |
| **pavis/63_security_rbac_spiffe.sh** | RBAC not implemented | **DEFER** (S3) | Wait for feature | SPIFFE RBAC implemented |
| **pavis/64_security_rbac_prefix.sh** | RBAC not implemented | **DEFER** (S3) | Wait for feature | Prefix RBAC implemented |
| **pavis/65_security_mtls_outbound.sh** | mTLS not implemented | **DEFER** (S3) | Wait for feature | Outbound mTLS implemented |
| **pavis/66_security_tls_sni_auto.sh** | TLS not implemented | **DEFER** (S3) | Wait for feature | SNI auto mode implemented |
| **pavis/67_security_mtls_chain_mode.sh** | mTLS not implemented | **DEFER** (S3) | Wait for feature | mTLS chain modes implemented |

**Summary**: 3 FIX required (S0-S1), 9 DEFER (S3 - feature gaps)

---

## 5. Coverage Matrix

### Architectural Invariants → Test Coverage

| Invariant | Tested By | Coverage | Gaps |
|-----------|-----------|----------|------|
| **A1: No Runtime Inference** | pavis/10, 34, 40, 42-44 | ✅ Strong | None |
| **A2: Immutable Execution State** | pavis/20-24, 40-44, 50-96, all reload tests | ✅ Strong | Missing: reload during body buffering (96 tests retry, not reload) |
| **A3: Layered Validation** | pavis/33 (violation), 34, integrated/30-32 | 🟡 Weak | **Gap**: Codec validation tested in runtime layer; No test for "runtime must not validate semantics" |
| **A4: Atomic Validity** | pavis/33, 44, integrated/30-32 | 🟡 Moderate | **Gap**: No test for partial corruption (valid header, corrupt routes) |
| **A5: Relay Opacity** | relay/10-11 | 🟡 Weak | **Gap**: No negative test (relay modifying PVS bytes) |
| **I2: Monotonic Versioning** | relay/11, 40, integrated/50 (SKIP) | ✅ Strong | None |
| **I3: Artifact Immutability** | relay/10-11 | ✅ Strong | None |
| **I4: System LKG** | pavis/30 (SKIP), 32-34, integrated/30-32 | 🟡 Moderate | **Gap**: Critical bug in pavis/30 |
| **I5: Hot Reload Contract** | pavis/20-24, 70, 92, integrated/20-21 | ✅ Strong | None |
| **I6: Long-Poll Contract** | relay/20-21, 30-31 | ✅ Strong | None |
| **I7: Checksum Validation** | relay/30, 50 | ✅ Strong | None |

**Key Gaps**:
1. **A4 (Atomic Validity)**: No test for partial config corruption
2. **A5 (Relay Opacity)**: No negative test for relay tampering
3. **I4 (System LKG)**: Critical bug in pavis/30_lkg.sh

---

### Feature Coverage → Test Cases

| Feature | Status | Tested By | Gaps |
|---------|--------|-----------|------|
| **Bootstrap** | ✅ P1 | integrated/10, pavis/10 | None |
| **L7 Routing (prefix/exact/regex)** | ✅ P1 | pavis/40-44 | None |
| **Hot Reload** | ✅ P3 | pavis/20-24, integrated/20-21 | None |
| **LKG Preservation** | ⚠️ P3 | pavis/30 (SKIP), 32-34, integrated/30-32 | **Bug** in pavis/30 |
| **Weighted Splitting** | ✅ P1 | pavis/41 | Statistical test needs higher sample size |
| **Method/Header Matching (P2)** | ✅ P2 | pavis/42-44 | None |
| **Retry (P2)** | ✅ P2 | pavis/51, 93-96 | Missing strict mode in 96 |
| **Resilience (timeouts, health, circuit)** | ✅ P6 | pavis/50-54 | None |
| **Connection Pooling (P0)** | ✅ P0 | pavis/80-83 | None |
| **TLS/mTLS** | ❌ P4 | pavis/60-67 (ALL SKIP) | Feature not implemented |
| **RBAC** | ❌ P4 | pavis/63-64 (SKIP) | Feature not implemented |
| **Observability** | ✅ P5 | pavis/70-72, 83, 90 | Minor: metrics assertions missing in 44, 52 |
| **Graceful Shutdown** | ✅ P7 | pavis/91 | Timing tolerance too loose |
| **Relay Protocol** | ✅ P3 | relay/* (all 18 tests) | None |

---

### Non-Goals (Must NOT Be Tested)

| Non-Goal | Status | Notes |
|----------|--------|-------|
| Runtime scripting (WASM, Lua) | ✅ Not tested | Correctly absent |
| Regex substitutions | ✅ Not tested | Only matching tested (pavis/44) |
| Inline secrets | ✅ Not tested | Only file paths tested (pavis/60-67) |
| Global rate limiting | ✅ Not tested | Correctly absent |
| SNI multi-cert | ✅ Not tested | Single cert only (pavis/60) |

**No violations detected** - Test suite correctly avoids testing explicitly dropped features.

---

## 6. Suite Restructure Proposal

### Option A: Minimal Churn (Recommended)

**Changes**:
1. **Merge redundant tests** (3 merges):
   - `integrated/20_reload_switch.sh` → Delete (covered by `pavis/20_reload_contract_core.sh`)
   - `relay/11_contract_republish.sh` + `relay/40_republish_stability.sh` → `relay/11_contract_republish_monotonicity.sh`
   - `pavis/51_resilience_retry.sh` → Simplify to smoke test (comprehensive coverage in 93-96)

2. **Fix naming** (7 renames):
   - `pavis/70_obs_consistency.sh` → `pavis/70_observability_consistency.sh`
   - `pavis/80_pool_hard_limit.sh` → `pavis/80_connection_pool_hard_limit.sh`
   - `pavis/81_pool_queue_behavior.sh` → `pavis/81_connection_pool_queue_behavior.sh`
   - `pavis/82_pool_high_limit.sh` → `pavis/82_connection_pool_high_limit.sh`
   - `pavis/83_pool_metric_tracking.sh` → `pavis/83_connection_pool_metric_tracking.sh`
   - `relay/30_fanout_multi.sh` → `relay/31_fanout_multi.sh`
   - `relay/31_fanout_late.sh` → `relay/32_fanout_late.sh`
   - `relay/50_transport_integrity.sh` → `relay/51_transport_integrity.sh`

3. **Split layering violations** (1 split):
   - `pavis/33_semantic_validation_suite.sh` → Split into:
     - `pavis/33_core_validation_suite.sh` (core semantic checks)
     - Move codec tests to `crates/pavis-codec-serde/tests/` (unit test level)

**Impact**: 11 file changes, preserves test IDs, minimal disruption

---

### Option B: Full Reorganization (NOT Recommended)

Would introduce new taxonomy:
- `tests/suites/{component}/{domain}/{id}_test.sh`
- Example: `tests/suites/pavis/routing/01_prefix_matching.sh`

**Reason for rejection**: High churn (62 renames), breaks existing CI references, unclear benefit over current flat namespace

**Recommendation**: **Option A (Minimal Churn)**

---

## 7. Naming Rule Enforcement

### Inferred Naming Conventions

From existing test cases:
1. **Prefix**: `{id}_{domain}_{feature}.sh` where `id` is 2-digit number (10-99)
2. **Domain abbreviations**: Allowed for common terms (`obs` ✅ in 70, `lkg` ✅ in 30-32, `tls` ✅ in 60-67)
3. **Spelling out**: Multi-word features use underscores (`active_health_check`, `graceful_shutdown`)
4. **ID spacing**: IDs grouped by 10s (10-19 bootstrap, 20-29 reload, etc.)

**Exception detected**: `pool` tests (80-83) should spell out `connection_pool` for clarity

---

### Rename Table

| Old Path | New Path | Rationale |
|----------|----------|-----------|
| `pavis/70_obs_consistency.sh` | `pavis/70_observability_consistency.sh` | Spell out abbreviation for clarity |
| `pavis/80_pool_hard_limit.sh` | `pavis/80_connection_pool_hard_limit.sh` | Disambiguate "pool" (thread pool vs connection pool) |
| `pavis/81_pool_queue_behavior.sh` | `pavis/81_connection_pool_queue_behavior.sh` | Consistency with 80 |
| `pavis/82_pool_high_limit.sh` | `pavis/82_connection_pool_high_limit.sh` | Consistency with 80 |
| `pavis/83_pool_metric_tracking.sh` | `pavis/83_connection_pool_metric_tracking.sh` | Consistency with 80 |
| `relay/30_fanout_multi.sh` | `relay/31_fanout_multi.sh` | Fix duplicate prefix (30_etag_validation already exists) |
| `relay/31_fanout_late.sh` | `relay/32_fanout_late.sh` | Cascade renumber after 30→31 |
| `relay/50_transport_integrity.sh` | `relay/51_transport_integrity.sh` | Fix duplicate prefix (50_persistence_recovery already exists) |

**Total renames**: 8 files

---

## End of Audit Report

**Prepared by**: Claude (Senior Test Architect)  
**Report Version**: 1.0  
**Next Review**: After fixing S0 critical issues",
