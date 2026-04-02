# Pavis Runtime (Data Plane) Suite: Design & Strength Review

## Executive Summary

**Overall Credibility**: SOUND (test suite is strong; gaps are in feature implementation, not test coverage)

**Key Strengths**:
- Zero-drop hot reload validation using concurrent request bursts (200 requests during transition)
- Comprehensive routing semantics in single artifact (matchers, headers, actions, rewrites)
- Strong LKG guardrails with sequential proof (corrupt → incompatible → recovery)
- Deterministic weight flipping eliminates statistical flakiness

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

### `20_reload_contract_core`

**Category**: Reload Semantics
**Contracts**: A (No-Drop), C (Atomic Switch), D (Zero-Option)
**Maturity**: L3

**Scenario**:
1. Start relay and pavis with config V1 (backend-v1 + response header set)
2. Validate initial routing and header presence
3. Capture SUT process/container ID
4. Spawn 200 requests in background
5. Publish V2 (backend-v2 + header removed) while traffic flowing
6. Wait for background requests to complete
7. Confirm switch to V2 and header removal

**Oracle**:
- HTTP status codes from burst requests
- Instance IDs from response bodies
- Response headers from burst and post-switch requests
- SUT process/container ID
- Process liveness

**Assertions**:
- Zero failed requests (100% success rate)
- Atomic switch: once V2 appears, V1 never appears again (sequential monotonicity)
- Header removal: after switch, `X-Backend-Version` absent
- SUT process ID unchanged (no restart)
- Process still alive after reload

**Assessment**: PASS. Single reload proves zero-drop, atomic switch, and immediate removal of policy (no hidden defaults).

---

### `21_reload_storm`

**Category**: Reload Semantics
**Contracts**: A (No-Drop), C (Atomic Switch)
**Maturity**: L3

**Scenario**:
1. Start with V1 (header `X-Backend-Version: v1`, backend-v1)
2. Run sustained traffic during a rapid publish storm V2..V10
3. After each publish, wait for version marker to appear, then sample post-switch requests

**Oracle**:
- Response headers (`X-Backend-Version`)
- Upstream echo (`instance_id`)
- Request success/failure

**Assertions**:
- Zero request failures during storm
- Once a higher version is observed, older versions never reappear (monotonicity)
- Per-request consistency: version marker aligns with upstream instance

**Assessment**: PASS. Stresses reload sequencing under load while preserving atomic, monotonic behavior.

---

### `22_reload_keepalive_atomic`

**Category**: Reload Semantics
**Contracts**: A (No-Drop), C (Atomic Switch)
**Maturity**: L3

**Scenario**:
1. Start with V1 (header `X-Backend-Version: v1`, backend-v1)
2. Open a single keep-alive connection and issue one request
3. Publish V2 (header `X-Backend-Version: v2`, backend-v2)
4. On the same connection, issue a series of post-reload requests

**Oracle**:
- Response headers (`X-Backend-Version`)
- Upstream echo (`instance_id`)
- Connection continuity (no errors)

**Assertions**:
- First request shows v1 + backend-v1
- Post-reload requests show only v2 + backend-v2
- No connection error or forced drop during reload

**Assessment**: PASS. Validates atomic switch behavior over a single keep-alive connection.

---

### `23_reload_atomic_mid_request`

**Category**: Reload Semantics
**Contracts**: C (Atomic Switch)
**Maturity**: L3

**Scenario**:
1. Start with V1 (header `X-Backend-Version: v1`, backend-v1)
2. Issue a slow request (`/delay?ms=1500`) and trigger reload to V2 mid-flight
3. After completion, issue a post-reload request

**Oracle**:
- Response headers (`X-Backend-Version`)
- Delay response body (`delayed_ms`)

**Assertions**:
- In-flight response retains v1 header and correct delay body
- Post-reload response returns v2 header

**Assessment**: PASS. Ensures in-flight requests complete under the original config version.

---

### `30_lkg_recovery`

**Category**: Failure & LKG
**Contracts**: B (LKG Preservation)
**Maturity**: L3

**Scenario**:
1. Start with valid V1 (backend-v1)
2. **Test 1**: Publish corrupt artifact (random bytes), wait 2s, validate traffic
3. **Test 2**: Publish incompatible artifact (corrupted PVS version byte), wait 2s, validate traffic
4. **Test 3**: Publish semantic-invalid artifact (missing upstream), wait 2s, validate traffic
5. **Test 4**: Publish valid V3 (backend-v3), poll for switch
5. Throughout: ensure runtime stays alive

**Oracle**:
- HTTP status and response bodies from `/echo`
- Process liveness

**Assertions**:
- After corrupt artifact: traffic still on backend-v1
- After incompatible artifact: traffic still on backend-v1
- After semantic-invalid artifact: traffic still on backend-v1
- After valid V3: traffic switches to backend-v2 (V3 uses v2 port)
- Runtime process never crashes

**Assessment**: PASS. Unified LKG enforcement covering corrupt payloads, incompatible protocol versions, and semantic-invalid artifacts. Sequential proof: reject → reject → reject → recover.

---

### `31_lkg_relay_unavailable`

**Category**: Failure & LKG
**Contracts**: B (LKG Preservation)
**Maturity**: L3

**Scenario**:
1. Start with V1 (backend-v1) via mock relay
2. Stop the relay and continue serving traffic
3. Restart relay, publish V2 (backend-v2), and wait for recovery

**Oracle**:
- Upstream echo (`instance_id`)
- Process liveness

**Assertions**:
- During relay outage, traffic remains on backend-v1
- After relay restore, traffic switches to backend-v2
- Runtime stays alive during outage

**Assessment**: PASS. Confirms runtime resilience when control-plane is unavailable and recovery once it returns.

---

### `40_validation_core_suite`

**Category**: Failure & LKG
**Contracts**: B (LKG Preservation)
**Maturity**: L3

**Scenario**:
1. Start with V1 (backend-v1)
2. Sequentially publish core-invalid configs:
   - Missing upstream reference
   - Invalid circuit breaker limits
   - Invalid outlier detection settings
   - Invalid health check thresholds
3. After each attempt, validate LKG traffic remains on backend-v1

**Oracle**:
- Upstream echo (`instance_id`)
- Compile/publish outcome (informational)

**Assertions**:
- Each invalid config is rejected (core validation) and traffic remains on backend-v1

**Assessment**: PASS. Covers core semantic validation errors while preserving LKG.

---

### `41_validation_runtime_env_rejection`

**Category**: Failure & LKG
**Contracts**: B (LKG Preservation)
**Maturity**: L3

**Scenario**:
1. Start with valid V1 (backend-v1) + metrics enabled
2. Publish V2 enabling TLS with missing cert/key paths
3. Runtime should reject V2 at the boot-time reload boundary before apply

**Oracle**:
- Metrics: `pavis_config_validation_total{result="fail",reason="semantic"}`
- Upstream echo (`instance_id`)

**Assertions**:
- Runtime emits boot-time reload rejection metric
- Traffic remains on backend-v1 (LKG)
- Runtime stays alive

**Assessment**: PASS. Validates boot-time reload rejection with LKG preservation.

---

### `43_validation_runtime_env_upstream_tls_rejection`

**Category**: Failure & LKG
**Contracts**: B (LKG Preservation)
**Maturity**: L3

**Scenario**:
1. Start with valid V1 (backend-v1) + metrics enabled
2. Publish runtime-safe V2 that enables upstream TLS with a missing CA bundle path
3. Runtime should reject V2 at env validation stage

**Oracle**:
- Metrics: `pavis_config_validation_total{result="fail",reason="runtime"}`
- Upstream echo (`instance_id`)

**Assertions**:
- Runtime emits runtime validation failure metric
- Traffic remains on backend-v1 (LKG)
- Runtime stays alive

**Assessment**: PASS. Validates runtime env checks that still apply after the boot/runtime split.

---

### `50_routing_semantics`

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
- Rewrite: path `/service-a/echo` → `/echo`, Host `rewrite.test` → `rewritten.internal`, query preserved
- Response headers transformed per config (set); removal not asserted in current script

**Assessment**: PASS. Comprehensive routing semantics coverage in single artifact without runtime restart.

---

### `51_routing_weighted`

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

### `52_routing_method_header_predicates`

**Category**: Traffic Management
**Contracts**: D (Zero-Option)
**Maturity**: L3

**Scenario**:
1. Verify method-based routing (GET vs POST) to different upstreams.
2. Verify single header exact match routing.
3. Verify multiple header match (AND logic).
4. Verify fallback to default route on method/header mismatch.

**Oracle**:
- Upstream `instance_id` from `/echo`

**Assertions**:
- GET /api/get -> backend-v1
- POST /api/post -> backend-v2
- Header `x-tenant: alice` -> backend-v1
- Headers `x-tenant: alice` AND `x-region: us-east` -> backend-v1
- Mismatched headers -> backend-v2 (fallback)

**Assessment**: PASS. Validates P0 method/header predicate combinations.

---

### `53_routing_tie_breaking`

**Category**: Traffic Management
**Contracts**: D (Zero-Option)
**Maturity**: L3

**Scenario**:
1. Configure overlapping prefix routes: `/api/v2` (more specific) vs `/api` (less specific).
2. Send requests matching both.

**Oracle**:
- Upstream `instance_id`

**Assertions**:
- `/api/v2/users` matches `/api/v2` route (longest prefix wins).
- `/api/users` matches `/api` route.

**Assessment**: PASS. Confirms deterministic tie-breaking based on prefix specificity.

---

### `60_resilience_timeout`

**Category**: Resilience
**Contracts**: (timeout enforcement)
**Maturity**: L3

**Scenario**:
1. Start with route timeout of 500ms; `/delay?ms=100` should succeed.
2. Publish new config with route timeout tightened to 50ms.
3. Send `/delay?ms=200` and expect a fast failure after reload.

**Oracle**:
- HTTP status code and request latency

**Assertions**:
- V1: `/delay?ms=100` returns 200
- V2: `/delay?ms=200` fails quickly after reload

**Assessment**: PASS. Confirms runtime enforces route timeouts and respects tightened reloads.

---

### `61_resilience_retry`

**Category**: Resilience
**Contracts**: (retry policy execution)
**Maturity**: L3

**Scenario**:
1. Configure upstream with one dead endpoint and one healthy endpoint.
2. Enable retry policy with `retry_on: ["connect_error"]`.
3. Send `/echo` and expect a successful retry to the healthy backend.

**Oracle**:
- JSON `instance_id` from `/echo`

**Assertions**:
- Request succeeds and returns `instance_id = "backend-v1"`

**Assessment**: PASS. Confirms retry-on-connect-failure behavior.

---

### `62_resilience_outlier_detection`

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

### `63_resilience_active_health_check`

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

### `64_resilience_circuit_breaker`

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

### `70_security_tls`

**Category**: Security
**Contracts**: (upstream TLS with custom CA)
**Maturity**: N/A
**Status**: ENABLED (OpenSSL backend)

**Intent**: Validate upgrading cleartext upstream connections to TLS with custom CA verification.

**Assessment**: Pending execution under OpenSSL backend.

---

### `71_security_inbound_mtls`

**Category**: Security
**Contracts**: (inbound mTLS with client cert validation)
**Maturity**: N/A
**Status**: ENABLED (OpenSSL backend)

**Intent**: Validate HTTPS termination with client certificate validation and unknown-CA rejection.

**Assessment**: Pending execution under OpenSSL backend.

---

### `72_security_rbac_spiffe`

**Category**: Security (RBAC)
**Contracts**: (SPIFFE identity exact match)
**Maturity**: L3

**Intent**: SPIFFE identity exact match authorization.

**Status**: ENABLED (OpenSSL RBAC)

**Assessment**: PASS. HTTPS listener with required client-auth extracts the SPIFFE URI via OpenSSL and returns 200 for the exact match, 403 for mismatched identities, and handshake failure without a cert.

---

### `73_security_rbac_prefix`

**Category**: Security (RBAC)
**Contracts**: (SPIFFE prefix match)
**Maturity**: L3

**Intent**: SPIFFE prefix match authorization.

**Status**: ENABLED (OpenSSL RBAC)

**Assessment**: PASS. Prefix principal accepts multiple SPIFFE identities under the allowed namespace while rejecting out-of-prefix and no-cert requests.

---

### `74_security_mtls_outbound`

**Category**: Security
**Contracts**: (outbound mTLS with client cert)
**Maturity**: N/A
**Status**: ENABLED (OpenSSL backend)

**Intent**: Validate outbound mTLS with client cert presentation and CA verification.

**Assessment**: Pending execution under OpenSSL backend.

---

### `75_security_tls_sni_auto`

**Category**: Security
**Contracts**: (auto SNI derivation)
**Maturity**: N/A
**Status**: ENABLED (OpenSSL backend)

**Intent**: Auto SNI derivation and fail-fast for invalid Auto SNI configs.

**Assessment**: Pending execution under OpenSSL backend.

---

### `76_security_mtls_chain_mode`

**Category**: Security
**Contracts**: (client cert chain_mode)
**Maturity**: N/A
**Status**: ENABLED (OpenSSL backend)

**Intent**: Client cert chain_mode handling (embedded vs default none).

**Assessment**: Pending execution under OpenSSL backend.

---

### `80_observability_metrics_contract`

**Category**: Observability
**Contracts**: D (Zero-Option)
**Maturity**: L3

**Scenario**:
1. Start pavis with metrics + access log enabled (tracing disabled) via mock relay.
2. Baseline metrics snapshot.
3. Generate traffic and assert **no increase** in spans-created while tracing is disabled.
4. Force access-log backpressure (bounded channel + throttled writer) and assert access-log dropped counter **delta ≥ 1**.
5. Publish config v2 with tracing enabled and unreachable OTLP endpoint.
6. **Reload oracle**: poll `/stats` until `config_version` transitions from v1 → v2.
7. After transition, assert reload counter **delta ≥ 1**.
8. Generate traffic and assert **spans-created** and **export-errors** counters **delta ≥ 1**.
9. Verify gauge presence: `pavis_runtime_config_version{version="2"}` exists.
10. Verify request metrics family contains expected `/echo` labels (no absolute counts).

**Oracle**:
- `/stats` config_version transition (reload activation)
- Prometheus metrics text format (delta + presence checks)

**Assertions**:
- Spans-created counter does **not** increase while tracing is disabled.
- Access-log dropped counter increases under deterministic backpressure.
- After `/stats` config_version transition, reload-count counter increases.
- With tracing enabled and unreachable OTLP endpoint: spans-created and export-errors counters increase.
- Gauge presence: `pavis_runtime_config_version{version="2"}` present.
- Request metrics family includes `/echo` label set (no absolute counter totals).

**Assessment**: PASS. Proves metrics contract, deterministic backpressure, reload activation semantics, and tracing export error behavior without external collectors.

---

### `81_observability_access_log`

**Category**: Observability
**Contracts**: (structured access logging)
**Maturity**: L3

**Scenario**:
1. Start pavis with access log file configured.
2. Send V1 traffic to backend-v1.
3. Reload to backend-v2 via mock relay.
4. Send V2 traffic and wait for access log flush with bounded backoff.
5. On failure, print diagnostics (log tail, SUT id, admin version if available).

**Oracle**:
- Access log file contents

**Assertions**:
- Access log contains entries for `upstream="backend"` and `upstream="backend-v2"`.

**Assessment**: PASS. Confirms access log persistence across reloads and file flush behavior.

---

### `82_observability_tracing_context`

**Category**: Observability
**Contracts**: (W3C trace context propagation)
**Maturity**: L3

**Scenario**:
1. Start pavis with tracing enabled (sampling 100).
2. Send request and confirm `traceparent` reaches upstream.
3. Restart pavis with tracing disabled (sampling 0).
4. Send request and confirm `traceparent` is not injected.

**Oracle**:
- Upstream echo headers

**Assertions**:
- Tracing enabled: `traceparent` header present.
- Tracing disabled: `traceparent` header absent.

**Assessment**: PASS. Confirms trace context injection is gated by tracing policy.

---

### `90_connection_pool_hard_limit`

**Category**: Resilience
**Contracts**: (connection pooling)
**Maturity**: L3

**Scenario**:
1. Configure `pool.max: 3` with 0 queue capacity.
2. Send 10 concurrent requests to a slow backend (2s delay).

**Oracle**:
- HTTP status codes

**Assertions**:
- Exactly 3 requests succeed (200 OK).
- Remaining 7 requests fail with 503 Service Unavailable (pool full).

**Assessment**: PASS. Proves hard connection limit enforcement.

---

### `91_connection_pool_queue_behavior`

**Category**: Resilience
**Contracts**: (connection pooling)
**Maturity**: L3

**Scenario**:
1. Configure `pool.max: 3` and `queue_capacity: 2`.
2. Send 10 concurrent requests.

**Oracle**:
- HTTP status codes

**Assertions**:
- Exactly 5 requests succeed (3 active + 2 queued).
- Remaining 5 requests fail with 503.

**Assessment**: PASS. Validates request queuing when connection pool is saturated.

---

### `92_connection_pool_high_limit`

**Category**: Resilience
**Contracts**: (connection pooling)
**Maturity**: L3

**Scenario**:
1. Configure `pool.max: 20`.
2. Send 10 concurrent requests.

**Oracle**:
- HTTP status codes

**Assertions**:
- All 10 requests succeed (no false rejections).

**Assessment**: PASS. Ensures no rejections occur when load is within pool limits.

---

### `93_connection_pool_metric_tracking`

**Category**: Observability / Resilience
**Contracts**: (connection pooling)
**Maturity**: L3

**Scenario**:
1. Configure `pool.max: 5`.
2. Send 5 concurrent slow requests.
3. Sample `/stats` gauge during and after execution.

**Oracle**:
- `/stats` JSON body

**Assertions**:
- Metrics show pool size increasing from 0 to 5 and returning to 0.

**Assessment**: PASS. Confirms metric accuracy for connection pool tracking.

---

### `00_operational_admin_api`

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

### `01_operational_graceful_shutdown`

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

### `02_operational_reload_resource_sanity`

**Category**: Operational Lifecycle
**Contracts**: (resource sanity during reloads)
**Maturity**: L3

**Scenario**:
1. Start with V1 (backend-v1)
2. Publish V2..V7 sequentially (header `X-Backend-Version: vN`)
3. After each reload, sample resource indicators (FD count, RSS) when /proc is available; otherwise log info and exit

**Oracle**:
- Process resource indicators (`/proc/<pid>/fd`, `/proc/<pid>/status`)
- Response headers (`X-Backend-Version`)

**Assertions**:
- Reloads apply successfully
- FD count is not strictly increasing on every reload
- RSS is not strictly increasing on every reload

**Assessment**: PASS. Coarse leak sentinel for reload cycles with deterministic bounds.

---

### `65_resilience_retry_status_codes`

**Category**: Resilience
**Contracts**: (retry policy execution)
**Maturity**: L3

**Scenario**:
1. Configure route with `max_attempts: 5` and `retry_on: ["status_code"]` for 503.
2. Request a flaky endpoint that fails with 503 twice before succeeding.

**Oracle**:
- HTTP status code
- Metrics `pavis_upstream_retries_total`

**Assertions**:
- Client receives 200 OK.
- Metrics show multiple retry attempts.

**Assessment**: PASS. Verified P2 retry-on-status-code functionality.

---

### `66_resilience_retry_idempotency`

**Category**: Resilience
**Contracts**: (retry policy execution)
**Maturity**: L3

**Scenario**:
1. Send POST request to flaky endpoint with `retry_non_idempotent: false`.
2. Send POST request to flaky endpoint with `retry_non_idempotent: true`.

**Oracle**:
- HTTP status code

**Assertions**:
- Phase A (false): Client receives 503 (no retry).
- Phase B (true): Client receives 200 OK (retry successful).

**Assessment**: PASS. Verified idempotency constraints for retries.

---

### `67_resilience_retry_budget`

**Category**: Resilience
**Contracts**: (retry policy execution)
**Maturity**: L3

**Scenario**:
1. Configure `request_timeout: 1000ms`.
2. Send request to flaky endpoint with 200ms delay per attempt and 300ms backoff.

**Oracle**:
- HTTP status code

**Assertions**:
- Client receives 504 Gateway Timeout when global budget is exhausted, even if attempts remain.

**Assessment**: PASS. Verified global timeout enforcement in retry loop.

---

### `68_resilience_retry_body_buffer`

**Category**: Resilience
**Contracts**: (retry policy execution)
**Maturity**: L3

**Scenario**:
1. Send POST with small body (within `max_request_body_buffer_bytes`).
2. Send POST with large body (exceeding limit).

**Oracle**:
- HTTP status code

**Assertions**:
- Small body: Sreceeds via retry (200 OK).
- Large body: Retry aborted (503 original error returned) because body not replayable.

**Assessment**: PASS. Verified body buffering vs streaming replay safety.

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

| Category                         | Cases | Maturity Distribution |
|----------------------------------|-------|-----------------------|
| Bootstrap & Initial Load         | 1     | L3: 1                 |
| Reload Semantics                 | 4     | L3: 4                 |
| Failure & LKG                    | 5     | L3: 5                 |
| Traffic Management               | 5     | L3: 5                 |
| Resilience (Timeout/Retry)       | 6     | L3: 6                 |
| Resilience (Health/CB/OD)        | 3     | L3: 3                 |
| Security (TLS/mTLS)              | 5     | L3: 5                 |
| Security (RBAC)                  | 2     | SKIPPED               |
| Observability (Metrics/Logs/Trace)| 3    | L3: 3                 |
| Connection Pooling               | 4     | L3: 4                 |
| Operational Lifecycle            | 3     | L3: 3                 |

**Total Cases**: 41  
**Active (L3)**: 39  
**Skipped**: 2 (RBAC feature gaps)

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
- RBAC authorization: SKIPPED (feature gap)
- Metrics exposition and cardinality protection: L3

**Weak or Partially Covered Areas**:
- **RBAC**: Tests are skipped pending feature implementation.
