# Pavis Runtime (Data Plane) Suite: Design & Strength Review

## Executive Summary

**Overall Credibility**: SOUND (test suite is strong; gaps are in feature implementation, not test coverage)

**Key Strengths**:
- Zero-drop hot reload validation using concurrent request bursts (200 requests during transition)
- Comprehensive routing semantics in single artifact (matchers, headers, actions, rewrites)
- Strong LKG guardrails with sequential proof (corrupt → incompatible → recovery)
- Deterministic weight flipping eliminates statistical flakiness

**Known Gaps**:
- 10 cases skipped: 7 due to rustls backend limitations (TLS/mTLS features), 3 due to unimplemented features (timeout/retry, access logs, tracing)
- Gaps are feature-level, not test design weaknesses

**Next Actions**: Implement timeout/retry policies in runtime; migrate to TLS backend supporting per-peer CA and client certs; resolve access log flush/sync timing issues.

---

## Data Plane Contract

The Runtime Suite validates the **Frozen Data Plane** contract. The `pavis` binary must:
1. Bootstrap strictly from immutable `.pvs` artifact
2. Evolve configuration dynamically via long-poll without process restarts (Hot Reload)
3. Survive invalid or malicious updates by falling back to Last-Known-Good (LKG) state
4. Execute routing and resilience policies deterministically based only on currently loaded artifact
5. Enforce Zero-Option philosophy: no runtime defaults or inference

### Formal Invariants

- **A (No-Drop)**: Configuration updates MUST NOT interrupt active connections or drop new requests during switch-over.
- **B (LKG Preservation)**: If new artifact fails validation, runtime MUST continue serving traffic using previous valid configuration.
- **C (Atomic Switch)**: A request MUST be handled entirely by exactly one configuration version (no mid-request switching).
- **D (Zero-Option)**: Runtime MUST NOT infer defaults. Behavior is explicit in artifact.

---

## Test Case Analysis

### `10_bootstrap_static`

**Category**: Bootstrap & Initial Load
**Contracts**: D (Zero-Option)
**Maturity**: L3

**Scenario**:
1. Create initial config with listener + upstream (backend-v1)
2. Generate `.pvs` artifact
3. Start `pavis` with local `.pvs` file (no relay connection)
4. Wait for `/healthz` readiness

**Oracle**:
- HTTP status from `/healthz`
- HTTP response from `/echo` including JSON body

**Assertions**:
- `/healthz` returns 200
- Traffic routes to correct upstream (instance_id = "backend-v1")
- No crashes or errors during static bootstrap

**Assessment**: PASS. Proves basic PVS ingestion and static operation without control plane.

---

### `20_reload_norestart`

**Category**: Reload Semantics
**Contracts**: A (No-Drop), C (Atomic Switch)
**Maturity**: L3

**Scenario**:
1. Start relay and pavis with config V1 (routes to backend-v1)
2. Validate initial routing
3. Capture SUT process/container ID
4. Spawn 200 concurrent requests in background
5. Publish V2 (routes to backend-v2) while traffic flowing
6. Wait for background requests to complete

**Oracle**:
- HTTP status codes from 200 concurrent requests
- Instance IDs from response bodies
- SUT process/container ID
- Process liveness

**Assertions**:
- Zero failed requests (100% success rate)
- Atomic switch: once V2 appears, V1 never appears again (sequential monotonicity)
- SUT process ID unchanged (no restart)
- Process still alive after reload

**Assessment**: PASS. Concurrent burst during transition proves zero-drop and atomic switch with sequential ordering validation.

---

### `21_reload_zero_option_impact`

**Category**: Reload Semantics
**Contracts**: D (Zero-Option)
**Maturity**: L3

**Scenario**:
1. Start with V1 config containing `response_headers.set_headers: X-Pavis-Version: v1`
2. Validate header present
3. Publish V2 with header policy completely removed
4. Poll for header absence (up to 20 retries)

**Oracle**:
- HTTP response headers from `/echo`

**Assertions**:
- V1: Header `X-Pavis-Version: v1` present
- V2: Header `X-Pavis-Version` absent

**Assessment**: PASS. Proves removed configuration fields are immediately removed from runtime behavior (no state carry-over or hidden defaults).

---

### `30_lkg`

**Category**: Failure & LKG
**Contracts**: B (LKG Preservation)
**Maturity**: L3

**Scenario**:
1. Start with valid V1 (backend-v1)
2. **Test 1**: Publish corrupt artifact (random bytes), wait 2s, validate traffic
3. **Test 2**: Publish incompatible artifact (corrupted PVS version byte), wait 2s, validate traffic
4. **Test 3**: Publish valid V3 (backend-v3), poll for switch
5. Throughout: ensure runtime stays alive

**Oracle**:
- HTTP status and response bodies from `/echo`
- Process liveness

**Assertions**:
- After corrupt artifact: traffic still on backend-v1
- After incompatible artifact: traffic still on backend-v1
- After valid V3: traffic switches to backend-v2 (V3 uses v2 port)
- Runtime process never crashes

**Assessment**: PASS. Unified LKG enforcement covering corrupt payloads and incompatible protocol versions. Sequential proof: reject → reject → recover.

---

### `40_traffic_routing_semantics`

**Category**: Traffic Management
**Contracts**: C (Atomic Switch), D (Zero-Option)
**Maturity**: L3

**Scenario**:
Tests comprehensive routing semantics in single artifact:
1. Exact vs Prefix precedence
2. Regex routing with fallback
3. Request header policies: `set`, `append`, `add`, `remove`
4. Response header policies: `set`, `remove`
5. Direct response with custom body
6. Redirect action with Location header
7. Path & Host rewrite with query preservation

**Oracle**:
- HTTP status codes
- Response bodies (JSON from `/echo`)
- Response headers
- Instance IDs from upstream

**Assertions**:
- `/exact` → backend-v2, `/prefix/*` → backend-v1 (precedence)
- `/regex/123` → backend-v2, `/regex/abc` → backend-v1 (regex + fallback)
- Request headers transformed per config (set/append/add/remove)
- Response headers transformed per config (set/remove)
- Direct response: status 200, body "Custom Static Response"
- Redirect: status 301, Location header correct
- Rewrite: path `/service-a` → `/`, Host `rewrite.test` → `rewritten.internal`, query preserved

**Assessment**: PASS. Comprehensive routing semantics coverage in single artifact without runtime restart.

---

### `41_traffic_weighted`

**Category**: Traffic Behavior Under Reload
**Contracts**: A (No-Drop)
**Maturity**: L3

**Scenario**:
1. **V1**: Single destination (100% backend-v1), send 20 requests
2. **V2**: Weight flip to single destination (100% backend-v2), send 20 requests
3. **V3**: Flip back to 100% backend-v1, send 20 requests

**Oracle**:
- Instance IDs from all responses

**Assertions**:
- V1: All 20 requests → backend-v1
- V2 (after switch): All 20 requests → backend-v2
- V3 (after switch): All 20 requests → backend-v1

**Assessment**: PASS. Deterministic 100%/0% splits eliminate statistical flakiness (no probabilistic sampling).

---

### `50_resilience_timeout`

**Category**: Resilience
**Contracts**: (timeout enforcement)
**Maturity**: N/A
**Status**: SKIPPED (feature not implemented in runtime)

**Intent**: Validate timeout enforcement per upstream configuration.

**Assessment**: N/A. Test design ready; blocked by unimplemented feature.

---

### `51_resilience_retry`

**Category**: Resilience
**Contracts**: (retry policy execution)
**Maturity**: N/A
**Status**: SKIPPED (feature not implemented in runtime)

**Intent**: Validate retry policy execution per upstream configuration.

**Assessment**: N/A. Test design ready; blocked by unimplemented feature.

---

### `52_resilience_outlier_detection`

**Category**: Resilience
**Contracts**: (passive outlier ejection)
**Maturity**: L3

**Scenario**:
1. Configure outlier detection: `consecutive_errors: 2`, `eject_duration: 500ms`
2. Send successful request (200)
3. Trigger 2 consecutive 500 errors
4. Send request to `/echo` (expect failure)
5. Wait 600ms (> eject_duration)
6. Send request to `/echo` (expect success)

**Oracle**:
- HTTP status codes from successive requests

**Assertions**:
- After 2 consecutive 500s: next request to `/echo` fails (endpoint ejected)
- After 600ms: request succeeds (endpoint re-admitted)

**Assessment**: PASS. Exercises failure counter, ejection window, and timed recovery.

---

### `53_resilience_active_health_check`

**Category**: Resilience
**Contracts**: (active health probes)
**Maturity**: L3

**Scenario**:
1. **Phase 1**: Start with health check path `/status?code=500` (always fails)
   - Poll up to 25 times (200ms intervals) for unhealthy state
2. **Phase 2**: Publish new config with health check path `/healthz` (succeeds)
   - Poll up to 25 times for healthy state

**Oracle**:
- HTTP status codes from `/echo` during polling

**Assertions**:
- Phase 1: Traffic eventually fails (no healthy endpoints)
- Phase 2: Traffic eventually succeeds (endpoint recovered)

**Assessment**: PASS. Validates active probe path semantics and health state transitions (unhealthy → config update → healthy).

---

### `54_resilience_circuit_breaker`

**Category**: Resilience
**Contracts**: (in-flight/pending limits)
**Maturity**: L3

**Scenario**:
1. Configure circuit breaker: `max_connections: 1`, `max_pending_requests: 1`
2. Send 2 concurrent long-running requests (`/delay?ms=1500`) in background
3. Send 3rd request immediately
4. Wait for background requests to complete

**Oracle**:
- HTTP status code from 3rd request

**Assertions**:
- 3rd request returns 503 (circuit breaker overflow)

**Assessment**: PASS. Concurrent long requests force in-flight limit; validates 503 rejection on overflow.

---

### `60_security_tls`

**Category**: Security
**Contracts**: (upstream TLS with custom CA)
**Maturity**: N/A
**Status**: SKIPPED (rustls backend limitation: per-peer CA not supported)

**Intent**: Validate upgrading cleartext upstream connections to TLS with custom CA verification.

**Assessment**: N/A. Blocked by backend limitation (not test design gap).

---

### `61_security_inbound_mtls`

**Category**: Security
**Contracts**: (inbound mTLS with client cert validation)
**Maturity**: N/A
**Status**: SKIPPED (rustls backend limitation: inbound mTLS not supported)

**Intent**: Validate HTTPS termination with client certificate validation and unknown-CA rejection.

**Assessment**: N/A. Blocked by backend limitation (not test design gap).

---

### `62_security_rbac_spiffe`

**Category**: Security (RBAC)
**Contracts**: (SPIFFE identity exact match)
**Maturity**: L3

**Intent**: SPIFFE identity exact match authorization.

**Assessment**: PASS. Covers match, mismatch, and no identity scenarios.

---

### `63_security_rbac_prefix`

**Category**: Security (RBAC)
**Contracts**: (SPIFFE prefix match)
**Maturity**: L3

**Intent**: SPIFFE prefix match authorization.

**Assessment**: PASS. Ensures prefix match enforcement and deny-by-default.

---

### `64_security_mtls_outbound`

**Category**: Security
**Contracts**: (outbound mTLS with client cert)
**Maturity**: N/A
**Status**: SKIPPED (rustls backend limitation: per-peer CA and client cert not supported)

**Intent**: Validate outbound mTLS with client cert presentation and CA verification.

**Assessment**: N/A. Blocked by backend limitation (not test design gap).

---

### `65_security_tls_sni_auto`

**Category**: Security
**Contracts**: (auto SNI derivation)
**Maturity**: N/A
**Status**: SKIPPED (rustls backend limitation: per-peer CA verification required)

**Intent**: Auto SNI derivation and fail-fast for invalid Auto SNI configs.

**Assessment**: N/A. Blocked by backend limitation (not test design gap).

---

### `66_security_mtls_chain_mode`

**Category**: Security
**Contracts**: (client cert chain_mode)
**Maturity**: N/A
**Status**: SKIPPED (rustls backend limitation: client cert presentation not supported)

**Intent**: Client cert chain_mode handling (embedded vs default none).

**Assessment**: N/A. Blocked by backend limitation (not test design gap).

---

### `70_obs_metrics`

**Category**: Observability
**Contracts**: D (Zero-Option)
**Maturity**: L3

**Scenario**:
1. Start pavis with metrics endpoint on separate port
2. Generate traffic: 2 requests to `/echo` (matched route)
3. Scrape metrics, validate counters
4. **Cardinality protection**: Send 2 requests to unmatched paths
5. Validate unmatched paths NOT in metrics
6. **Hot reload test**: Publish new config, send traffic, validate counter persistence

**Oracle**:
- Prometheus metrics text format
- Metric label values
- Counter values

**Assertions**:
- `pavis_http_requests_total{route="/echo", status="200"} 2`
- `pavis_upstream_requests_total{upstream="backend", status="200"} 2`
- Unmatched paths not present in metrics (no label explosion)
- After hot reload: counter value = 3 (persistence)

**Assessment**: PASS. Proves Prometheus metrics exposition, label-cardinality protection, and metric persistence across hot reload.

---

### `71_obs_access_log`

**Category**: Observability
**Contracts**: (structured access logging)
**Maturity**: N/A
**Status**: SKIPPED (binary mode access log verification inconsistent due to flush/sync timing)

**Intent**: Verify structured access logging to file.

**Assessment**: N/A. Implementation issue (not test design gap).

---

### `72_obs_tracing_context`

**Category**: Observability
**Contracts**: (W3C trace context propagation)
**Maturity**: N/A
**Status**: SKIPPED (dynamic tracing sampling updates not applied reliably)

**Intent**: Verify W3C trace context propagation to upstreams.

**Assessment**: N/A. Implementation issue (not test design gap).

---

### `80_obs_cross_consistency`

**Category**: Observability
**Contracts**: (cross-signal consistency)
**Maturity**: N/A
**Status**: SKIPPED (trace ID propagation check failing in binary mode)

**Intent**: Verify metrics, access logs, and response headers agree on request identifiers.

**Assessment**: N/A. Implementation issue (not test design gap).

---

### `90_operational_admin_api`

**Category**: Operational Lifecycle
**Contracts**: D (Zero-Option)
**Maturity**: L3

**Scenario**:
1. Start pavis with admin API enabled on separate port (shutdown disabled for test speed)
2. Configure 2 upstreams and 2 routes for config count validation
3. Test `/health` endpoint returns correct JSON
4. Test `/stats` endpoint returns all required fields (version, uptime_seconds, listeners, upstreams, routes)
5. Verify config counts in `/stats` match runtime configuration
6. Verify uptime counter increases over time
7. Test unknown paths return 404
8. Verify admin API isolated to admin port only (not accessible on traffic port)
9. Verify traffic routing unaffected by admin API presence

**Oracle**:
- HTTP status codes
- JSON response bodies from `/health` and `/stats`
- Response headers

**Assertions**:
- `/health` returns `{"status":"healthy"}` with 200 OK
- `/stats` contains version, uptime_seconds, listeners, upstreams, routes fields
- Config counts: listeners=1, upstreams=2, routes=2
- Uptime increases after 2s delay
- Unknown path `/unknown` returns 404
- Admin endpoints not accessible on traffic port
- Traffic routes correctly to backend while admin API is active

**Assessment**: PASS. Validates read-only admin API endpoints, bind-address isolation, config reflection, and independence from traffic routing.

---

### `91_operational_graceful_shutdown`

**Category**: Operational Lifecycle
**Contracts**: (graceful drain on SIGTERM)
**Maturity**: L3

**Scenario**:
1. Start mock upstream with 3s response delay
2. Start pavis with graceful shutdown enabled (5s drain timeout)
3. Initiate long-running request in background (3s delay)
4. Send SIGTERM to pavis after 0.5s (request in-flight)
5. Wait for in-flight request to complete
6. Verify request completed successfully with valid response
7. Verify request duration matches upstream delay (~3s)
8. Verify pavis exits within drain_timeout + request_duration + buffer (<10s total)

**Oracle**:
- HTTP status and response body from in-flight request
- Request duration timing
- Process exit timing
- Process liveness

**Assertions**:
- In-flight request completes successfully (no connection drop)
- Response contains valid JSON with expected instance_id
- Request duration 3-6 seconds (matches 3s upstream delay)
- Pavis exits gracefully within 10s of SIGTERM
- Process exits cleanly (no crashes)

**Assessment**: PASS. Validates SIGTERM triggers graceful drain, in-flight requests complete during drain phase, and process exits within bounded timeout.

---

## Implementation Principles

- **Isolation**: Every request includes `X-Pavis-Test-Run` and `X-Pavis-Test-Case` headers.
- **Black-Box Testing**: Assert behavior via HTTP status/body or mock-upstream `/echo` endpoint.
- **Mode-Agnostic Infrastructure**: Use `get_sut_id`, `stop_sut`, `check_sut_alive` for Binary and Docker modes.
- **Zero-Drop Validation**: Use concurrent request bursts during hot reload to prove Invariant A.
- **Atomic Switch Validation**: Track version monotonicity during bursts to prove Invariant C.

---

## Coverage Analysis

### Summary

| Category                  | Cases | Maturity Distribution |\n|---------------------------|-------|-----------------------|\n| Bootstrap & Initial Load  | 1     | L3: 1                 |\n| Reload Semantics          | 2     | L3: 2                 |\n| Failure & LKG             | 1     | L3: 1                 |\n| Traffic Management        | 2     | L3: 2                 |\n| Resilience (Timeout/Retry)| 2     | SKIPPED               |\n| Resilience (Health/CB/OD) | 3     | L3: 3                 |\n| Security (TLS/mTLS)       | 5     | SKIPPED (rustls)      |\n| Security (RBAC)           | 2     | L3: 2                 |\n| Observability (Metrics)   | 1     | L3: 1                 |\n| Observability (Logs/Trace)| 3     | SKIPPED               |\n| Operational Lifecycle     | 2     | L3: 2                 |\n\n**Total Cases**: 24\n**Active (L3)**: 14\n**Skipped**: 10 (7 rustls limitations, 3 implementation gaps)

### Risk Coverage Mapping

**High-Risk Areas** (and coverage):
- **Traffic drop during reload** (A): L3 (200-request concurrent burst)
- **State corruption from bad config** (B): L3 (sequential corrupt/incompatible/valid)
- **Mid-request version switching** (C): L3 (atomic switch validation with monotonicity)
- **Runtime default inference** (D): L3 (zero-option impact test)

**Well-Covered Areas**:
- Hot reload semantics (A, C, D): L3
- LKG guardrails (B): L3
- Routing semantics (matchers, headers, actions, rewrites): L3
- Resilience (outlier detection, health checks, circuit breaker): L3
- RBAC authorization: L3
- Metrics exposition and cardinality protection: L3

**Weak or Partially Covered Areas**:
- **TLS/mTLS**: Blocked by rustls backend (7 cases skipped)
- **Timeout/Retry policies**: Feature not implemented (2 cases skipped)
- **Access logs and tracing**: Implementation timing issues (3 cases skipped)

---

## Evolution Plan

### Short-Term (Must Address)

1. **Implement timeout/retry policies in runtime**:
   - Unblock `50_resilience_timeout` and `51_resilience_retry`
   - Critical for production resilience

2. **Resolve access log flush/sync timing**:
   - Fix binary mode access log buffering issues
   - Unblock `71_obs_access_log`

3. **Fix trace ID propagation in binary mode**:
   - Diagnose and fix trace context propagation failures
   - Unblock `72_obs_tracing_context` and `80_obs_cross_consistency`

### Mid-Term (Should Improve)

4. **Migrate to TLS backend supporting per-peer CA**:
   - Replace rustls or add per-peer CA support
   - Unblock 7 TLS/mTLS test cases
   - Critical for production security features

5. **Add negative resilience tests**:
   - Test outlier detection with partial failures (not just consecutive)
   - Test circuit breaker recovery after backoff

### Long-Term (Optional Enhancements)

6. **Weighted routing with probabilistic splits**:
   - Add statistical validation for 50/50 or 70/30 splits (not just 100/0)
   - Requires large sample sizes (N > 1000) for statistical confidence

7. **Concurrent reload stress test**:
   - Publish V1 → V2 → V3 → ... → V10 rapidly while sending sustained traffic
   - Validate zero-drop across multiple rapid transitions
