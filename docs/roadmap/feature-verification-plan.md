# Feature Verification Implementation Plan

> **Status**: This document tracks deferred P0/P1/P2 items that have **not yet been implemented**.
>
> **Completed**: The following P0 features are now **fully implemented** and documented in the main roadmap:
> - ✅ **Header/Method Routing Gap** (P0 Item #1) - Method and header predicates with multiple header support
> - ✅ **Upstream pool.max Enforcement** (P0 Item #2) - Connection caps with queue management
>
> See `docs/roadmap/roadmap.md` for the complete feature status.

---

## Error Taxonomy (Implemented ✅)

All validation and runtime errors use structured error codes with typed fields as implemented in `pavis-core/src/error.rs`.

**Implementation Reference**:
- Error codes: `pavis-core/src/error.rs` (lines 5-24)
- Field path builder: `pavis-core/src/error.rs` (lines 63-118)
- Canonical format enforced across codec, core, and runtime layers

---

## E2E Case Integration and Documentation Maintenance (Implemented ✅)

### Integration-First Policy

E2E test coverage follows the integration-first approach as documented.

**Implementation Reference**:
- Routing E2E tests: `tests/suites/pavis/42_routing_method_header_predicates.sh`, `43_routing_tie_breaking.sh`
- Pool E2E tests: `tests/suites/pavis/80_pool_hard_limit.sh` through `83_pool_metric_tracking.sh`
- Integration tests: `crates/pavis-codec-serde/tests/codec_integration.rs` (lines 237-564)

---

## Deferred Items (Roadmap TODO)

The following items are intentionally deferred to future milestones and are NOT implemented.

### 3) Inbound mTLS (rustls) Blocked [DEFERRED]

#### Contract

**Accepted Behavior**:
- Routes match when ALL specified predicates are satisfied (conjunction).
- **Cardinality**: P0 allows **multiple header predicates** per route, treated as AND across predicates.
  - Example: route requires `X-Tenant: alice` AND `X-Region: us-east` → both must match.
  - Single method predicate per route (P0); multiple methods are P2.
- Evaluation order: host → path → method → headers (short-circuit on first mismatch).
- Method matching: case-sensitive, exact match against HTTP method enum.
- Header matching:
  - Header name: case-insensitive (per HTTP spec).
  - Header value: case-sensitive, exact match.
  - **Multiple header predicates**: AND across predicates (all must match).
  - Multi-value headers: match if ANY value matches (OR within a single header).
  - Missing header: predicate fails (no implicit empty string).
  - Empty header value: treated as literal empty string, distinct from missing.
- Tie-breaking: if multiple routes match all predicates, first route in config order wins.
- Default: if no method/header predicates specified, those dimensions are wildcards (always match).

**Rejected Behavior**:
- Codec MUST reject invalid HTTP methods (not in standard enum) with `ERR_INVALID_CONFIG` (field_path="routes[N].match.method").
- Codec MUST reject empty header names with `ERR_INVALID_CONFIG` (field_path="routes[N].match.headers[...]", constraint="header_name_non_empty").
- Runtime MUST NOT infer or guess missing predicates.

#### Ownership

| Layer   | Responsibility |
|---------|----------------|
| Codec   | Parse method/header selectors from config; validate method names and header names; materialize wildcards as explicit "MatchAny" variants; reject malformed predicates with `ERR_INVALID_CONFIG` |
| Core    | Define matcher model with explicit enums (`MethodMatcher::Exact(Method)`, `HeaderMatcher::ExactValue { name, value }`); support multiple header predicates (Vec); provide evaluation semantics (ordering, short-circuit, AND across predicates) |
| Runtime | Execute matcher predicates in defined order; apply AND logic across multiple header predicates; no semantic inference; return first matching route or 404 |

#### Implementation Notes

- **Pingora Integration**: extend `pingora_proxy::upstream_peer` selection logic to apply predicates before peer selection.
- **Evaluation Order**: host/path already exist; method/header checks added after path but before peer selection.
- **Short-Circuit**: stop evaluation on first predicate failure to minimize overhead.
- **Case Normalization**: header names normalized to lowercase at codec layer for comparison.
- **Multiple Header Predicates**: core matcher model uses `Vec<HeaderMatcher>` and applies AND logic; all predicates must succeed.

#### Observability / Verdict Signals

- **Metrics** (runtime):
  - `pavis_route_match_attempts_total{result="matched|no_match"}`: counter per route.
  - `pavis_route_match_predicate_failures_total{predicate_type="method|header"}`: counter per predicate type.
- **Logs** (trace level):
  - "Route candidate X rejected: method mismatch (expected GET, got POST)".
  - "Route candidate Y rejected: header 'X-Foo' missing".
  - "Route candidate Z rejected: header 'X-Tenant' value mismatch (expected 'alice', got 'bob')".
- **Test Verdict**: route selection is deterministic; assert exact upstream target, not just 200 vs 404.

#### Flake Avoidance Rule

**Prohibited Assertions**:
- Throughput thresholds (e.g., "at least 1000 req/s").
- Latency percentiles (e.g., "p99 < 50ms").
- Timing-sensitive assertions (e.g., "response within 100ms").

**Required Assertions**:
- Stable error codes/fields for rejections.
- Metric counters/gauges (exact values or deterministic bounds).
- Response status codes and exact upstream targets.
- Deterministic route selection based on request predicates.

#### Tests

**Unit Tests** (core matcher logic):
1. Single method predicate: match GET, reject POST.
2. Single header predicate: match exact value, reject missing header.
3. Single header predicate: match exact value, reject different value.
4. Multi-value header: match if any value matches (OR within header).
5. **Multiple header predicates**: `X-Tenant: alice` AND `X-Region: us-east` → both match, route selected; one fails → route rejected.
6. **Multiple header predicates**: `X-Tenant: alice` AND `X-Debug: true` → first matches, second missing → route rejected.
7. Compound predicates (path + method): match both, reject if either fails.
8. Compound predicates (path + method + multiple headers): match all, reject if any fails.
9. Evaluation order: verify short-circuit (mock counters to prove method checked before headers).
10. Case sensitivity: header name case-insensitive, value case-sensitive.
11. Empty vs missing header: empty string matches literal "", missing fails predicate.

**Integration Tests** (codec → core):
1. Parse valid method selector → `MethodMatcher::Exact(Method::GET)`.
2. Parse invalid method name → `ERR_INVALID_CONFIG` with `field_path="routes[0].match.method"`.
3. Parse header selector with empty name → `ERR_INVALID_CONFIG` with `field_path="routes[0].match.headers[...]"`, `constraint="header_name_non_empty"`.
4. Parse missing method/header selectors → materialize as `MatchAny` variants.
5. **Parse multiple header predicates** → `Vec<HeaderMatcher>` with correct name/value pairs.

**E2E Tests** (full runtime):

Prefer integrating these verifications into existing routing E2E cases where feasible. If creating new cases, document invariant and rationale.

1. **Method Routing** (case: `routing_method_selection`):
   - Two routes, same path, different methods (GET vs POST).
   - GET request → route A, upstream A responds.
   - POST request → route B, upstream B responds.
   - PUT request → 404 (no route matches).
   - Assert: response status codes, upstream targets (not timing).

2. **Header Routing** (case: `routing_header_selection`):
   - Two routes, same path, different headers (`X-Tenant: alice` vs `X-Tenant: bob`).
   - Request with `X-Tenant: alice` → route A.
   - Request with `X-Tenant: bob` → route B.
   - Request with `X-Tenant: charlie` → 404.
   - Request with missing `X-Tenant` → 404.
   - Assert: response status codes, upstream targets.

3. **Multiple Header Predicates** (case: `routing_multiple_headers`):
   - Route requires `X-Tenant: alice` AND `X-Region: us-east`.
   - Request with both headers matching → route A.
   - Request with `X-Tenant: alice`, missing `X-Region` → 404.
   - Request with `X-Region: us-east`, missing `X-Tenant` → 404.
   - Request with `X-Tenant: alice` AND `X-Region: eu-west` → 404 (wrong region).
   - Assert: response status codes, `pavis_route_match_predicate_failures_total{predicate_type="header"}` increments on failures.

4. **Compound Predicates** (case: `routing_compound_predicates`):
   - Route requires path `/api` AND method GET AND header `X-Tenant: alice`.
   - All match → route A.
   - Path + method match, header fails → 404.
   - Path + header match, method fails → 404.
   - Assert: response status codes, metric counters.

5. **Multi-Value Header** (case: `routing_multivalue_header`):
   - Route matches `Accept: application/json`.
   - Request with `Accept: text/html, application/json` → matches (OR within header).
   - Request with `Accept: text/html, application/xml` → 404.
   - Assert: response status codes.

6. **Tie-Breaking** (extend existing case or create `routing_tie_breaking`):
   - Two routes with identical predicates (path + method + headers).
   - All requests → first route in config order wins.
   - Assert: exact upstream target (e.g., "upstream-A" not "upstream-B").


#### Contract

**Accepted Behavior**:
- `pool.max` defines the maximum concurrent connections **per upstream peer** (not global, not per-worker).
- Valid range: `1..=u32::MAX` (at least 1, no upper bound enforcement in codec but runtime may clamp to practical limits).
- **Core Invariant**: Active upstream connections MUST NEVER exceed `pool.max` (hard limit, enforced before connection attempt).
- When limit reached: behavior depends on queue configuration (see below).
- Default: if `pool.max` unspecified in source config, codec materializes explicit default (e.g., 128).

**Queue and Overflow Semantics**:
To make E2E tests deterministic, the following queue parameters MUST be explicitly defined in config:
- `pool.queue_capacity`: max number of queued requests (e.g., 0 = no queue, 10 = queue up to 10).
- `pool.queue_timeout_ms`: max time a request waits in queue before 503 (e.g., 5000 = 5s timeout).

Behavior when `pool.max` reached:
- If `queue_capacity > 0` AND queue not full: enqueue request, wait up to `queue_timeout_ms`.
- If `queue_capacity = 0` OR queue full: return 503 immediately with `ERR_UPSTREAM_POOL_FULL`.
- If queued request exceeds `queue_timeout_ms`: return 503 with `ERR_UPSTREAM_POOL_FULL`.

**Rejected Behavior**:
- Codec MUST reject `pool.max = 0` with `ERR_INVALID_CONFIG` (field_path="upstreams[N].pool.max", constraint="min_value=1").
- Codec MUST reject `pool.max < 0` (signed type or overflow) with `ERR_INVALID_CONFIG`.
- Codec MUST reject `pool.queue_capacity < 0` with `ERR_INVALID_CONFIG`.
- Codec MUST reject `pool.queue_timeout_ms < 0` with `ERR_INVALID_CONFIG`.
- Runtime MUST NOT silently ignore or override the configured value.
- Runtime MUST NOT allow active connections to exceed `pool.max` (even transiently).

#### Ownership

| Layer   | Responsibility |
|---------|----------------|
| Codec   | Parse `pool.max`, `pool.queue_capacity`, `pool.queue_timeout_ms`; validate `pool.max >= 1` and queue params; materialize defaults if missing; reject invalid values with `ERR_INVALID_CONFIG` |
| Core    | Store `pool.max` as non-Option field (e.g., `NonZeroU32` or explicit `PoolMax` newtype); store queue params as non-Option fields; document per-peer scope and queue semantics |
| Runtime | Enforce `pool.max` hard limit (active connections never exceed); implement queue with capacity and timeout; emit metrics on active, queued, and rejected requests; return 503 with `ERR_UPSTREAM_POOL_FULL` on overflow/timeout |

#### Implementation Notes

- **Pingora Mapping Accuracy**:
  - **Investigation Required**: Identify the exact Pingora API that enforces concurrent connection caps (not idle connection limits).
  - If Pingora provides `ConnectionPool::max_connections` or equivalent: wire `pool.max` to this field.
  - If Pingora only provides `max_idle_connections` (does NOT cap concurrent): implement an explicit semaphore/permit gating layer:
    - Use `tokio::sync::Semaphore` with `pool.max` permits.
    - Acquire permit before upstream connection attempt; release on connection close.
    - If no permit available: check queue; enqueue or reject immediately.
  - **Documentation Requirement**: Implementation MUST document the exact Pingora field/API used OR the semaphore gating layer added.
  - **Test Requirement**: Verify `pavis_upstream_pool_size` gauge reflects concurrent connections (not idle connections).

- **Per-Peer Scope**: Pingora pools are per-peer; confirm mapping is 1:1 (one pool per upstream backend).

- **Queue Implementation**:
  - Use bounded channel (e.g., `tokio::sync::mpsc::channel(queue_capacity)`) or explicit queue data structure.
  - Track queue depth with gauge metric.
  - Apply `queue_timeout_ms` using `tokio::time::timeout` on queue wait.

- **Backend Split**: pool configuration is same for HTTP and TLS paths (no separate rustls vs OpenSSL logic).

#### Observability / Verdict Signals

- **Metrics** (runtime):
  - `pavis_upstream_pool_size{upstream}`: gauge of active connections (MUST reflect concurrent, not idle).
  - `pavis_upstream_pool_limit{upstream}`: gauge of configured `pool.max`.
  - `pavis_upstream_pool_queue_depth{upstream}`: gauge of queued requests.
  - `pavis_upstream_pool_queue_capacity{upstream}`: gauge of configured `pool.queue_capacity`.
  - `pavis_upstream_pool_rejections_total{upstream, reason="queue_full|queue_timeout"}`: counter of 503 due to pool exhaustion.
- **Logs** (warn level):
  - "Upstream pool full for 'backend-X', returning 503 (limit: N, active: N, queued: M, reason: queue_full)".
  - "Upstream pool queue timeout for 'backend-X', returning 503 (limit: N, active: N, queued: M, timeout: T ms)".
- **Test Verdict**:
  - `pavis_upstream_pool_size` gauge MUST NOT exceed `pool.max` under any load (core invariant).
  - `pavis_upstream_pool_rejections_total` MUST increment when queue full or timeout.
  - Use deterministic queue parameters to predict exact rejection counts.

#### Flake Avoidance Rule

**Prohibited Assertions**:
- Throughput thresholds (e.g., "at least 7 requests return 503" unless queue params make this deterministic).
- Latency-based assertions (e.g., "requests complete within 2s").
- Timing-sensitive waits (e.g., "sleep 100ms then check metric").

**Required Assertions**:
- `pavis_upstream_pool_size <= pool.max` at all times (sample metric repeatedly, assert invariant always holds).
- `pavis_upstream_pool_rejections_total` increments when queue exhausted (count-based, deterministic with known queue params).
- Response status codes (200 vs 503) based on queue availability.
- Metric gauge values (exact or bounded).

#### Tests

**Unit Tests** (codec validation):
1. `pool.max = 1` → accepted.
2. `pool.max = 1000` → accepted.
3. `pool.max = 0` → `ERR_INVALID_CONFIG` with `field_path="upstreams[0].pool.max"`, `constraint="min_value=1"`.
4. `pool.max = -5` → `ERR_INVALID_CONFIG` (if signed type allowed in DTO).
5. `pool.max` missing → materialized as explicit default (e.g., 128).
6. `pool.queue_capacity = -1` → `ERR_INVALID_CONFIG` with `field_path="upstreams[0].pool.queue_capacity"`, `constraint="min_value=0"`.
7. `pool.queue_timeout_ms = -100` → `ERR_INVALID_CONFIG` with `field_path="upstreams[0].pool.queue_timeout_ms"`, `constraint="min_value=0"`.

**Integration Tests** (core → runtime config):
1. `pool.max = 5` → runtime receives `PoolMax(5)` (non-Option).
2. Runtime inspects config: `pool.max`, `queue_capacity`, `queue_timeout_ms` are accessible and non-None.

**E2E Tests** (full runtime under load):

Prefer integrating these verifications into existing upstream connection cases where feasible. If creating new cases, document invariant and rationale.

1. **Capped Concurrency (No Queue)** (case: `upstream_pool_hard_limit`):
   - Config: `pool.max = 3`, `pool.queue_capacity = 0` (no queue, immediate rejection).
   - Backend delays responses by 2 seconds (simulate slow upstream).
   - Send 10 concurrent requests.
   - Assert: `pavis_upstream_pool_size` gauge MUST NOT exceed 3 at any sample point (poll every 100ms during test).
   - Assert: `pavis_upstream_pool_rejections_total{reason="queue_full"}` increments (queue disabled, immediate rejection).
   - Assert: Exactly 3 requests succeed (200 OK), exactly 7 requests receive 503 (deterministic, no queue).
   - Rationale: Verify hard limit enforcement and immediate rejection with no queue.

2. **Capped Concurrency with Queue** (case: `upstream_pool_queue_behavior`):
   - Config: `pool.max = 3`, `pool.queue_capacity = 2`, `pool.queue_timeout_ms = 5000`.
   - Backend delays responses by 2 seconds.
   - Send 10 concurrent requests.
   - Assert: `pavis_upstream_pool_size` MUST NOT exceed 3.
   - Assert: `pavis_upstream_pool_queue_depth` peaks at 2 (queue capacity).
   - Assert: Exactly 5 requests succeed (3 active + 2 queued), exactly 5 requests receive 503 (queue full).
   - Assert: `pavis_upstream_pool_rejections_total{reason="queue_full"}` = 5.
   - Rationale: Verify queue behavior and deterministic rejection when queue full.

3. **Increased Concurrency (No False Rejections)** (case: `upstream_pool_high_limit`):
   - Config: `pool.max = 20`, `pool.queue_capacity = 10`.
   - Backend delays responses by 2 seconds.
   - Send 10 concurrent requests.
   - Assert: All 10 requests succeed (200 OK).
   - Assert: `pavis_upstream_pool_size` peaks at 10 (no rejections, within limit).
   - Assert: `pavis_upstream_pool_rejections_total` = 0.
   - Rationale: Verify no false rejections when load is within limit.

4. **Metric Accuracy** (case: `upstream_pool_metric_tracking`):
   - Config: `pool.max = 5`, `pool.queue_capacity = 0`.
   - Send 5 concurrent slow requests (2s delay).
   - Poll metrics during execution (every 100ms):
     - `pavis_upstream_pool_size` = 5 (all slots occupied).
     - `pavis_upstream_pool_limit` = 5 (configured limit).
     - `pavis_upstream_pool_queue_depth` = 0 (no queue).
   - Wait for completion: `pavis_upstream_pool_size` returns to 0.
   - Rationale: Verify gauge accuracy and lifecycle tracking.


**Intent**: Reject unsupported inbound mTLS configurations when rustls backend is selected.

**Key Contract** (for future implementation):
- Codec detects combination: `tls.backend = rustls` AND `tls.inbound.client_ca` is configured.
- Codec rejects config with `ERR_UNSUPPORTED_FEATURE` (feature="inbound_mtls", backend="rustls").
- Rejection happens at codec layer before core receives config.
- Last Known Good (LKG) config is preserved; runtime never sees invalid config.
- Field path format: `tls.inbound.client_ca` (canonical format per Error Taxonomy).

**Status**: Not implemented in this milestone. Backend-specific validation gates will be added in a future milestone focused on TLS configuration hardening.

---

### 4) Outbound Custom CA (rustls) Blocked [DEFERRED]

**Intent**: Reject unsupported per-peer CA bundles when rustls backend is selected.

**Key Contract** (for future implementation):
- Codec detects combination: `tls.backend = rustls` AND `upstreams[*].tls.ca_bundle` is configured.
- Codec rejects config with `ERR_UNSUPPORTED_FEATURE` (feature="per_peer_ca_bundle", backend="rustls").
- Rejection happens at codec layer before core receives config.
- Last Known Good (LKG) config is preserved; runtime never sees invalid config.
- Field path format: `upstreams[N].tls.ca_bundle` (canonical format per Error Taxonomy).

**Status**: Not implemented in this milestone. Backend-specific validation gates will be added in a future milestone focused on TLS configuration hardening.

---

### 6) Validation Suite for Ignored Fields [DEFERRED]

**Intent**: Ensure configs that are parsed but ignored/blocked fail fast with precise structured errors.

**Objective** (for future implementation):
- Maintain explicit inventory of "parsed but unsupported" fields in `tests/ignored-fields.toml`.
- Generate E2E tests from inventory that assert rejection with exact error code + context fields.
- Ensure error codes and context fields are stable (breaking change to modify).
- Staleness detection: parse codec source to extract validation gates; compare with inventory; warn if mismatch.
- Use canonical field path format per Error Taxonomy.

**Status**: Not implemented in this milestone. This validation infrastructure will be added when backend-specific rejection gates (items 3 and 4) are implemented, providing a comprehensive test suite for all unsupported feature combinations.

---

## P2 – Feature Candidates (Deferred / Not in This Milestone)

**Status**: P2 items (7 and 8) are deferred to future milestones. They are documented here for planning purposes but are NOT implemented in this milestone.

### 7) Header/Method Routing Enhancements [DEFERRED]

**Goal**: expand matcher expressiveness beyond P0 minimal viable subset.

**Status**: Deferred to future milestone. P0 provides single method predicate and multiple header predicates with exact match; P2 adds multiple methods, header operators (prefix/regex/present), and compound boolean logic (AND/OR/NOT).

#### P0 vs P2 Boundary

**P0 (Minimal Viable)** (implemented in this milestone):
- Single method predicate per route: exact match.
- Multiple header predicates per route: exact value match, case-insensitive name, AND across predicates.
- Conjunction of predicates: host AND path AND method AND headers.
- No compound logic within a single predicate type.

**P2 (Incremental Enhancements)** (deferred):
- Multiple method predicates: `methods: [GET, POST]` (disjunction within methods).
- Header predicate operators: `exact`, `prefix`, `regex`, `present` (any value).
- Compound predicates: `(path = /foo AND method = GET) OR (path = /bar AND method = POST)`.
- Negation: `NOT (header X-Debug present)`.

(Contract, Ownership, Tests omitted - see future milestone plan.)

---

### 8) Route Retries/Timeouts Implementation (Full Policy) [DEFERRED]

**Goal**: wire full retry policy with per-try budgets and backoff.

**Status**: Deferred to future milestone. P0 provides global request timeout; P2 adds retry policy with per-try timeouts, backoff strategies, and retryable status codes.

#### P0 vs P2 Boundary

**P0 (Baseline)** (current state):
- Global request timeout (single deadline for entire request, including all retries).
- No retries (fail on first upstream error).

**P2 (Full Policy)** (deferred):
- Retry policy: max attempts, retryable status codes, backoff strategy.
- Per-try timeout: each attempt has independent deadline (within global budget).
- Backoff: fixed, linear, or exponential (configurable).
- Budget enforcement: global timeout encompasses all retries + backoff delays.

(Contract, Ownership, Tests omitted - see future milestone plan.)

---

## Appendix: Cross-Cutting Concerns

### Frozen Data Plane Compliance

All P0/P1/P2 features MUST adhere to the Frozen Data Plane contract:
- Runtime executes config deterministically (no learning, no adaptation).
- Codec layer resolves all defaults and policy decisions.
- Runtime emits metrics/logs for observability but does not modify behavior based on observations.

### Regression Prevention

- **Error Codes**: new validation gates MUST use structured error codes with typed context fields.
- **Field Path Format**: all errors MUST use canonical field path format (see Error Taxonomy).
- **Test Stability**: tests MUST assert on stable error codes/fields first, messages secondarily.
- **CI Enforcement**: regressions in test coverage or error contract stability MUST fail CI.

### Documentation Requirements

After implementing each feature:
- Update `docs/project/features.md` with status (✅/⏳/⚠️/❌).
- Update `ARCHITECTURE.md` if protocol/layering changes.
- Update `docs/project/roadmap.md` if milestones completed (refresh summary at top).
- Add code comments explaining "why" for complex logic.
- Update E2E suite design doc when tests are added/modified (see E2E Case Integration policy).
- Add test file comments explaining rationale for E2E test cases (invariant, verdict signals, rationale for new case vs extension).

---

## Summary of Deferred Work

The following items remain for future implementation:

- **Item 3**: Inbound mTLS (rustls) rejection gate
- **Item 4**: Outbound Custom CA (rustls) rejection gate
- **Item 6**: Validation suite for ignored fields
- **Item 7**: Header/Method Routing Enhancements (P2)
- **Item 8**: Route Retries/Timeouts Full Policy (P2)
