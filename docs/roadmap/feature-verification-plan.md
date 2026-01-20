# Feature Verification Implementation Plan (P0/P1/P2)

This plan expands the P0/P1/P2 items into concrete engineering steps with explicit
contracts, layer ownership, and deterministic test expectations.

**Milestone Scope**: This revision focuses on core routing and connection pool enforcement.
Backend-specific validation gates and capability matrices are deferred to future milestones.
**P2 items (7, 8) are deferred to future milestones and not implemented here.**

---

## Error Taxonomy

All validation and runtime errors MUST use structured error codes with typed fields.
Tests MUST assert on error codes and fields first, error messages secondarily.

### Error Code Structure

```rust
// Example structure (not implementation)
struct PavisError {
    code: ErrorCode,        // Stable enum variant
    context: ErrorContext,  // Structured fields
    message: String,        // Human-readable (unstable)
}

enum ErrorCode {
    ERR_UNSUPPORTED_FEATURE,
    ERR_INVALID_CONFIG,
    ERR_VALIDATION_FAILED,
    ERR_BACKEND_INCOMPATIBLE,
    ERR_UPSTREAM_POOL_FULL,
    // ...
}

struct ErrorContext {
    feature: Option<String>,
    backend: Option<String>,
    field_path: Option<String>,
    constraint: Option<String>,
    upstream: Option<String>,
    // ...
}
```

### Field Path Canonical Format

All error contexts MUST use a single canonical `field_path` syntax for referring to config fields.

**Canonical Format Rules**:
- Array indices: `routes[0]`, `upstreams[2]`
- Nested fields: `routes[0].match.method`, `upstreams[0].pool.max`
- Map keys (quoted): `routes[0].match.headers["x-tenant"]`, `upstreams[0].metadata["region"]`
- Header names in field paths: always lowercase, quoted: `routes[0].match.headers["accept"]`

**Examples**:
```
routes[0].match.method              // method field
routes[0].match.headers["x-tenant"] // header predicate
upstreams[0].pool.max               // pool max field
upstreams[0].tls.ca_bundle          // TLS CA bundle
listeners[0].bind_address           // listener address
```

**Requirements**:
- Codec, core, and runtime MUST emit `field_path` in this canonical format.
- Internal Rust struct field names (e.g., `bind_addr`, `max_connections`) are FORBIDDEN in `field_path`.
- Use user-facing config field names exactly as they appear in source config files.
- Tests MUST assert exact `field_path` strings using this format.

### Test Assertions

```rust
// ✅ GOOD: Assert on stable code + fields
assert_eq!(error.code, ErrorCode::ERR_INVALID_CONFIG);
assert_eq!(error.context.field_path, Some("routes[0].match.method"));
assert_eq!(error.context.constraint, Some("valid_http_method"));

// ✅ GOOD: Header field path (lowercase, quoted)
assert_eq!(error.context.field_path, Some("routes[0].match.headers[\"x-tenant\"]"));

// ❌ BAD: Internal Rust field name
assert_eq!(error.context.field_path, Some("routes[0].matcher.http_method"));

// ⚠️ SECONDARY: Message can change
assert!(error.message.contains("invalid method"));
```

---

## E2E Case Integration and Documentation Maintenance

### Integration-First Policy

When adding E2E test coverage, prefer integrating verification into existing cases over creating new ones.

**Integration Decision Tree**:
1. **First**: Identify existing E2E case that exercises the relevant dimension (routing selection, upstream pool behavior, TLS handshake, etc.).
2. **Attempt**: Extend the existing case with additional assertions or request variations.
3. **Guard**: Verify extension does not break the case's original invariant or introduce flakiness.
4. **Fallback**: If integration is not feasible (conflicting invariants, excessive complexity, cross-test interference), create a new case.

### Requirements for New E2E Cases

When creating a new E2E case is necessary, it MUST include:

1. **Invariant Name and Rationale**:
   - Clear, concise invariant statement (e.g., "pool.max enforces per-peer connection cap").
   - Explicit reason why it cannot fit an existing case (e.g., "existing routing cases do not exercise concurrent connection limits").

2. **Deterministic Verdict Signals**:
   - Metric counters/gauges to assert (e.g., `pavis_upstream_pool_size <= pool.max`).
   - Log patterns to verify (e.g., "Upstream pool full" warning message).
   - Error codes/fields for rejection scenarios.
   - No assertions on throughput, latency percentiles, or timing-sensitive thresholds.

3. **No-Flake Assertion Strategy**:
   - State-based assertions: metric values, error codes, response status.
   - Avoid timing-based assertions (e.g., "request completes within 100ms").
   - Use controlled failure injection (mock upstreams with deterministic delays/responses).
   - Explicit synchronization points (wait for metric to reach expected value, not fixed sleep).

4. **Cleanup and Isolation**:
   - Explicit cleanup steps to reset runtime state (if needed).
   - Isolation requirements: dedicated upstream, unique route paths, non-overlapping ports.
   - Document any shared resources and synchronization mechanisms.

### Documentation Maintenance Requirements

When E2E tests are added or modified:

1. **Suite Design Documentation**:
   - Update E2E suite design doc (e.g., `tests/e2e/README.md` or `docs/testing/e2e-suite.md`) to list the case and its invariant.
   - Include case ID/name, invariant statement, dimensions exercised, and verdict signals.

2. **Plan Documentation**:
   - Update this plan's test list to reference the case ID/name (e.g., "E2E case `routing_method_header_compound`").
   - Ensure test count in summary matches actual implemented cases.

3. **Case Extension Documentation**:
   - If an existing case is extended, update its inline documentation (test file comments) to include the new verification dimension.
   - List all invariants/dimensions the case now covers.

4. **Single Source of Truth**:
   - Documentation MUST be the authoritative reference for what each case proves.
   - Code comments MUST reference the documented invariant, not duplicate it.

---

## P0 – Safety & Correctness (Active Implementation)

### 1) Header/Method Routing Gap

**Goal**: matcher supports method/header predicates in addition to path/host.

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

---

### 2) Upstream pool.max Ignored

**Goal**: enforce connection caps from config.

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

---

## Roadmap TODO (Deferred to Future Milestones)

The following items are intentionally skipped in this milestone and will be implemented later.

### 3) Inbound mTLS (rustls) Blocked [SKIPPED]

**Intent**: Reject unsupported inbound mTLS configurations when rustls backend is selected.

**Key Contract** (for future implementation):
- Codec detects combination: `tls.backend = rustls` AND `tls.inbound.client_ca` is configured.
- Codec rejects config with `ERR_UNSUPPORTED_FEATURE` (feature="inbound_mtls", backend="rustls").
- Rejection happens at codec layer before core receives config.
- Last Known Good (LKG) config is preserved; runtime never sees invalid config.
- Field path format: `tls.inbound.client_ca` (canonical format per Error Taxonomy).

**Status**: Not implemented in this milestone. Backend-specific validation gates will be added in a future milestone focused on TLS configuration hardening.

---

### 4) Outbound Custom CA (rustls) Blocked [SKIPPED]

**Intent**: Reject unsupported per-peer CA bundles when rustls backend is selected.

**Key Contract** (for future implementation):
- Codec detects combination: `tls.backend = rustls` AND `upstreams[*].tls.ca_bundle` is configured.
- Codec rejects config with `ERR_UNSUPPORTED_FEATURE` (feature="per_peer_ca_bundle", backend="rustls").
- Rejection happens at codec layer before core receives config.
- Last Known Good (LKG) config is preserved; runtime never sees invalid config.
- Field path format: `upstreams[N].tls.ca_bundle` (canonical format per Error Taxonomy).

**Status**: Not implemented in this milestone. Backend-specific validation gates will be added in a future milestone focused on TLS configuration hardening.

---

### 6) Validation Suite for Ignored Fields [SKIPPED]

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

## Remaining Scope Summary

This milestone focuses on two core P0 items that establish routing and connection pool enforcement foundations. **P2 items (7, 8) are deferred and not in scope.**

### Active Implementation Items

#### 1) Header/Method Routing Gap

**Contract (Key Points)**:
- Routes match when ALL predicates satisfied (host AND path AND method AND headers).
- **Multiple header predicates allowed** (AND across predicates); single method predicate (P0 limit).
- Evaluation order: host → path → method → headers (short-circuit).
- Method: case-sensitive exact match; Header name: case-insensitive; Header value: case-sensitive.
- Multi-value headers: match if ANY value matches (OR within header).
- Tie-breaking: first matching route in config order wins.

**Layer Ownership**:
- Codec: parse, validate, materialize wildcards, reject malformed predicates (use canonical field paths).
- Core: define matcher model with explicit enums, support Vec<HeaderMatcher>, document evaluation semantics.
- Runtime: execute predicates in order (AND across headers), no inference, deterministic route selection.

**Deterministic Verdict Signals**:
- Metrics: `pavis_route_match_attempts_total{result}`, `pavis_route_match_predicate_failures_total{predicate_type}`.
- Logs: trace-level rejection reasons with exact predicate mismatch details.
- Test verdict: assert exact upstream target (not just status code).

**Minimal Test Set**:
- **Unit**: 11 tests covering single/compound predicates, multiple headers (AND), evaluation order, case sensitivity, empty vs missing headers.
- **Integration**: 5 tests covering codec parsing, error code validation, multiple header predicates.
- **E2E**: 6 tests covering method/header routing, multiple header predicates (AND), multi-value headers (OR within), compound predicates, tie-breaking.

**Flake Avoidance**: No throughput/latency assertions; use deterministic status codes, metric counters, upstream targets.

---

#### 2) Upstream pool.max Ignored

**Contract (Key Points)**:
- `pool.max` = max concurrent connections per upstream peer (not global, not per-worker).
- **Core Invariant**: active connections MUST NEVER exceed `pool.max`.
- Valid range: `1..=u32::MAX`.
- Queue params: `pool.queue_capacity` (0+ requests), `pool.queue_timeout_ms` (0+ ms).
- Limit reached: enqueue if queue available, else 503 with `ERR_UPSTREAM_POOL_FULL`.
- Default: codec materializes explicit defaults if unspecified.

**Layer Ownership**:
- Codec: parse, validate `pool.max >= 1` and queue params, materialize defaults, reject invalid values (use canonical field paths).
- Core: store as non-Option fields (e.g., `NonZeroU32`), document per-peer scope and queue semantics.
- Runtime: enforce hard limit (semaphore if Pingora lacks concurrency cap), implement queue, emit metrics on active/queued/rejected.

**Pingora Mapping**:
- Identify exact Pingora API for concurrent connection caps (not idle limits).
- If unavailable: add explicit semaphore gating layer (document in implementation notes).
- Verify `pavis_upstream_pool_size` reflects concurrent connections (not idle).

**Deterministic Verdict Signals**:
- Metrics: `pavis_upstream_pool_size{upstream}` (gauge, MUST NOT exceed pool.max), `pavis_upstream_pool_limit`, `pavis_upstream_pool_queue_depth`, `pavis_upstream_pool_rejections_total{reason}`.
- Logs: warn-level pool full messages with exact limit/active/queued counts.
- Test verdict: gauge invariant (size <= max), rejection counter increments, deterministic 200 vs 503 counts based on queue params.

**Minimal Test Set**:
- **Unit**: 7 tests covering valid values, zero/negative rejection, default materialization, queue param validation.
- **Integration**: 2 tests covering non-Option field wiring.
- **E2E**: 4 tests covering hard limit (no queue), queue behavior, high limit (no false rejects), metric accuracy.

**Flake Avoidance**: No throughput/latency assertions; use deterministic queue params, poll metrics for invariant, assert exact counts based on config.

---

### Error Taxonomy (Cross-Cutting)

**Structured Error Codes**:
- `ERR_INVALID_CONFIG`: malformed config (invalid method, empty header name, pool.max < 1, queue params < 0).
- `ERR_UPSTREAM_POOL_FULL`: runtime pool limit reached (503 response, queue full or timeout).

**Field Path Canonical Format**:
- `routes[N].match.method`, `routes[N].match.headers["x-tenant"]`, `upstreams[N].pool.max`, `upstreams[N].pool.queue_capacity`.
- Use user-facing config field names, not internal Rust names.

**Test Strategy**:
- Assert error code + context fields first (stable contract).
- Assert field_path in canonical format.
- Assert message content secondarily (unstable, can change).

---

### Deferred Items (Roadmap TODOs)

- **Item 3**: Inbound mTLS (rustls) rejection gate → future TLS hardening milestone.
- **Item 4**: Outbound Custom CA (rustls) rejection gate → future TLS hardening milestone.
- **Item 6**: Validation suite for ignored fields → implemented with items 3/4 in future milestone.
- **Item 7**: Header/Method Routing Enhancements (P2) → future routing milestone.
- **Item 8**: Route Retries/Timeouts (P2) → future resilience milestone.

**No backend capability matrix, validation suite infrastructure, or P2 features in this milestone.**
