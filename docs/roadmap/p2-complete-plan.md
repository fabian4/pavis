## P2 – Designed Implementation Plans (Not Executed in This Milestone)

**Status**: P2 items (7 and 8) have complete, implementation-ready plans documented below. These plans are designed and approved for future execution but are NOT implemented in this milestone. P0 items (1 and 2) remain the only active implementation items.

---

### 7) Header/Method Routing Enhancements

**Goal**: expand matcher expressiveness beyond P0 minimal viable subset.

#### P0 vs P2 Boundary

**P0 (Minimal Viable)** (implemented in this milestone):
- Single method predicate per route: exact match.
- Multiple header predicates per route: exact value match, case-insensitive name, AND across predicates.
- Conjunction of predicates: host AND path AND method AND headers.
- No compound logic within a single predicate type.

**P2 (Incremental Enhancements)** (designed, not executed):
- Multiple method predicates: `methods: [GET, POST]` (disjunction within methods).
- Header predicate operators: `exact`, `prefix`, `regex`, `present` (any value).
- Compound predicates: `(path = /foo AND method = GET) OR (path = /bar AND method = POST)`.
- Negation: `NOT (header X-Debug present)`.

#### Contract

**Accepted Behavior**:

**Canonical field_path Format for Header Operators**:
- Header predicates use operator-qualified paths: `routes[N].match.headers["header-name"].{operator}`.
- Examples:
  - `routes[0].match.headers["x-tenant"].exact`
  - `routes[0].match.headers["x-tenant"].prefix`
  - `routes[0].match.headers["x-version"].regex`
  - `routes[0].match.headers["x-debug"].present`
- Header names in field paths: always lowercase, quoted.
- Errors MUST use this format (e.g., `ERR_INVALID_CONFIG` with `field_path="routes[0].match.headers["x-custom"].regex"`).

**Multi-Method Lists**:
- Config: `methods: [GET, POST, HEAD]` → match if request method is in the list (OR semantics).
- Empty list rejected by codec with `ERR_INVALID_CONFIG` (field_path="routes[N].match.methods", constraint="non_empty_list").
- Duplicate methods allowed (deduplicated at codec layer).

**Header Operators**:
- `exact`: value equals literal string (P0 behavior, case-sensitive).
  - Example: `header X-Tenant exact "alice"` matches `X-Tenant: alice`, rejects `X-Tenant: Alice`.
- `prefix`: value starts with literal string (case-sensitive).
  - Example: `header X-Tenant prefix "team-"` matches `X-Tenant: team-alpha`, rejects `X-Tenant: user-alpha`.
- `regex`: value matches regex pattern (deterministic NFA evaluation with strict input/pattern limits).
  - Example: `header X-Version regex "v[0-9]+"` matches `X-Version: v123`, rejects `X-Version: vABC`.
  - Regex engine: Rust `regex` crate (linear-time NFA, deterministic evaluation).
  - **Pattern Limits** (enforced by codec):
    - `features.routing.regex_pattern_max_bytes`: max pattern string length.
      - Default: `256` bytes.
      - Valid range: `1..=4096`.
      - Enforcement: codec validates `pattern.len() <= regex_pattern_max_bytes`.
      - Failure mode: reject with `ERR_INVALID_CONFIG` (field_path="routes[N].match.headers["..."].regex", constraint="regex_pattern_too_long").
    - Pattern syntax validation: codec MUST attempt lightweight syntax check (e.g., `regex::bytes::Regex::new(pattern)`).
      - Failure mode: reject with `ERR_INVALID_CONFIG` (constraint="regex_invalid_syntax").
  - **Compilation Limits** (enforced by core/runtime):
    - `features.routing.regex_size_limit_bytes`: max compiled regex program size.
      - Default: `10485760` (10MB).
      - Valid range: `1048576..=104857600` (1MB to 100MB).
      - Enforcement: runtime compiles with `regex::bytes::RegexBuilder::new(pattern).size_limit(regex_size_limit_bytes).build()`.
      - Failure mode: compilation failure surfaces as `ERR_INVALID_CONFIG` during config loading (field_path="routes[N].match.headers["..."].regex", constraint="regex_size_limit_exceeded").
  - **Input Value Limits** (enforced by runtime):
    - `features.routing.regex_input_max_bytes`: max header value length for regex evaluation.
      - Default: `4096` bytes.
      - Valid range: `1..=1048576` (1 byte to 1MB).
      - Enforcement: runtime checks `header_value.len() <= regex_input_max_bytes` before evaluation.
      - Failure mode: treat predicate as deterministic non-match (not error), increment `pavis_route_match_regex_input_too_large_total`, log at debug level.
  - Compiled regex caching: runtime stores compiled `Arc<Regex>` in route config, reused across all requests (no per-request compilation).
- `present`: header exists with any non-empty value.
  - Example: `header X-Debug present` matches `X-Debug: true`, `X-Debug: 1`, `X-Debug: anything`.
  - Rejects if header missing or value is empty string.

**Compound Predicates (AND/OR/NOT AST)**:
- Explicit AST representation: `PredicateNode::And(Vec<PredicateNode>)`, `PredicateNode::Or(Vec<PredicateNode>)`, `PredicateNode::Not(Box<PredicateNode>)`.
- Evaluation order: depth-first, left-to-right within AND/OR nodes.
- Short-circuit: AND stops on first false; OR stops on first true.
- Tie-breaking: if multiple routes match compound predicates, first route in config order wins (same as P0).
- Default precedence: AND binds tighter than OR (but config syntax must use explicit parentheses/structure).

**Evaluation Order and Cost Heuristic**:
- Predicates ordered by cost before execution:
  - Cost: `exact = 1`, `prefix = 2`, `present = 1`, `regex = 10`.
  - Within same cost: preserve config order.
- Cheap predicates (exact, present, prefix) evaluated before expensive ones (regex).
- Short-circuit on first failure (within AND node) minimizes wasted computation.
- Cost ordering is an implementation optimization; correctness does not depend on it (order affects performance, not semantics).

**Tie-Breaking**:
- Same as P0: first matching route in config order wins.

**Rejected Behavior**:
- Codec MUST reject regex patterns exceeding byte length limit with `ERR_INVALID_CONFIG` (field_path="routes[N].match.headers["..."].regex", constraint="regex_pattern_too_long").
- Codec MUST reject regex patterns with invalid syntax with `ERR_INVALID_CONFIG` (field_path="routes[N].match.headers["..."].regex", constraint="regex_invalid_syntax").
- Runtime MUST reject regex patterns that fail compilation (size limit exceeded) with `ERR_INVALID_CONFIG` during config loading (field_path="routes[N].match.headers["..."].regex", constraint="regex_size_limit_exceeded").
- Codec MUST reject ambiguous compound predicates with unclear precedence (require explicit AST structure in config).
- Runtime MUST NOT infer or guess missing operators (default to `exact` if operator unspecified in config).
- Runtime MUST treat regex input exceeding `regex_input_max_bytes` as deterministic non-match (not error, not panic).

#### Ownership

| Layer   | Responsibility |
|---------|----------------|
| Codec   | Parse multi-method lists, header operators, compound predicate AST; validate regex pattern byte length (`<= regex_pattern_max_bytes`); validate regex syntax (lightweight, e.g., attempt parse); deduplicate method lists; materialize defaults (e.g., operator=exact if unspecified); reject malformed/oversized patterns with `ERR_INVALID_CONFIG`; pass pattern strings (not compiled regex) to core |
| Core    | Define extended matcher model (`MethodMatcher::AnyOf(Vec<Method>)`, `HeaderOperator` enum, `PredicateNode` AST); document evaluation semantics (order, short-circuit, cost heuristic); store regex pattern strings in matcher structures; provide limit configuration (`regex_pattern_max_bytes`, `regex_size_limit_bytes`, `regex_input_max_bytes`) |
| Runtime | Compile regex patterns with size limits (`RegexBuilder::size_limit(...)`); surface compilation failures as `ERR_INVALID_CONFIG` during config loading; cache compiled `Arc<Regex>` in route structures; execute AST with cost-aware ordering; evaluate regex predicates (check input length, use cached compiled regex); emit metrics on operator evaluations and input limit rejections; no semantic inference; deterministic route selection |

#### Implementation Notes

**Feature Flag Gating**:
- Config field: `features.routing.advanced_matchers` (boolean, default: `false`).
- If disabled: codec rejects routes with multi-method lists, non-exact operators, or compound predicates (ERR_INVALID_CONFIG, constraint="feature_disabled").
- If enabled: full P2 matcher capabilities available.

**Canonicalization Rules**:
- Header names: normalized to lowercase at codec layer (same as P0).
- Method names: normalized to uppercase at codec layer (GET, POST, etc.).
- Regex patterns: stored as-is (case sensitivity controlled by regex syntax, e.g., `(?i)` flag).

**Cost Heuristic Implementation**:
- Codec assigns cost metadata to each `HeaderMatcher` variant during parsing (stored in matcher model).
- Runtime sorts predicates by cost before evaluation (stable sort preserves config order within same cost).
- Cost values: use `MatcherCost(u8)` newtype to avoid magic numbers.
- Proof strategy: unit tests with mock counters verify evaluation order; E2E tests verify functionality only (not execution order).

**Regex Compilation and Caching**:
- Codec phase:
  - Validate pattern byte length: `pattern.len() <= features.routing.regex_pattern_max_bytes`.
  - Validate pattern syntax: attempt `regex::bytes::Regex::new(pattern)` (lightweight check).
  - On validation failure: reject with `ERR_INVALID_CONFIG` (field_path="routes[N].match.headers["..."].regex", constraint="regex_pattern_too_long" or "regex_invalid_syntax").
  - Store pattern string (not compiled regex) in core matcher structure.
- Runtime phase (config loading):
  - Compile pattern with size limit: `regex::bytes::RegexBuilder::new(pattern).size_limit(features.routing.regex_size_limit_bytes).build()`.
  - On compilation failure: surface as `ERR_INVALID_CONFIG` (field_path="routes[N].match.headers["..."].regex", constraint="regex_size_limit_exceeded").
  - Store compiled `Regex` in route structure (wrapped in `Arc<Regex>` for cloning/sharing).
- Runtime phase (request handling):
  - Check input length: `if header_value.len() > features.routing.regex_input_max_bytes { /* non-match */ }`.
  - Evaluate: `compiled_regex.is_match(header_value)` (deterministic, linear-time).
  - Cached regex reused across all requests (no recompilation per request).

**Determinism Preservation**:
- Rust `regex` crate uses linear-time NFA (no exponential backtracking).
- Regex evaluation is deterministic: same pattern + input = same result (always).
- Input length cap ensures bounded evaluation cost (no runaway computation).
- No timeouts needed (evaluation completes in deterministic time proportional to input length).

**Pingora Integration**:
- Same hook point as P0: extend `pingora_proxy::upstream_peer` selection logic.
- Matcher evaluation happens before peer selection (same pipeline stage).
- AST evaluation: recursive descent with short-circuit (no memoization needed for typical shallow trees).

**PII in Logs**:
- Trace logs MUST NOT log full header values (potential PII).
- Log format: "Route candidate X rejected: header 'X-Tenant' regex mismatch (pattern: 'team-.*')".
- Do NOT log: "header 'X-Tenant' value 'alice-personal-data' did not match".

#### Observability / Verdict Signals

**Metrics** (runtime):
- `pavis_route_match_predicate_evaluations_total{operator}`: counter of evaluations per operator type (operator="exact", "prefix", "regex", "present").
- `pavis_route_match_regex_input_too_large_total`: counter of regex input length limit rejections (global, no route label).
- `pavis_route_match_attempts_total{result}`: counter per result (result="matched", "no_match"; global, no route label).
- `pavis_route_match_predicate_failures_total{predicate_type}`: counter per predicate type (predicate_type="method", "header", "compound"; global, no route label).

**Cardinality Control**:
- Route identifiers MUST NOT be used as Prometheus labels (high cardinality, unbounded).
- Route information may appear only in debug/trace logs, not metrics.
- All metrics use low-cardinality labels: operator, predicate_type, result (bounded enums).

**Logs** (trace level):
- "Route candidate X rejected: method not in list (expected [GET, POST], got PUT)".
- "Route candidate Y rejected: header 'X-Tenant' prefix mismatch (expected prefix 'team-')".
- "Route candidate Z rejected: header 'X-Version' regex mismatch (pattern: 'v[0-9]+')".
- "Route candidate W rejected: header 'X-Debug' not present".

**Logs** (debug level):
- "Regex input too large for route=R, header='X-Custom', input_len=5000, limit=4096 (treated as non-match)".

**Test Verdict**:
- Route selection is deterministic (same request → same route, always).
- Assert exact upstream target (not just status code).
- Regex input limit → deterministic non-match (assert 404 or next route, assert counter increment).

**No PII in Logs**:
- Never log header values (only header names and pattern descriptions).
- Use structured logging with fields: `header_name`, `operator`, `pattern` (no `header_value`).

#### Flake Avoidance Rule

**Prohibited Assertions**:
- Throughput thresholds (e.g., "at least 1000 req/s").
- Latency percentiles (e.g., "p99 < 50ms").
- Timing-sensitive assertions (e.g., "regex completes within 5ms").
- Exact evaluation counts in E2E tests (e.g., "exactly 3 predicates evaluated") — use ">= 1" for counters in E2E.

**Required Assertions**:
- Stable error codes/fields for rejections (ERR_INVALID_CONFIG for oversized/invalid regex patterns).
- Metric counters (exact values in unit tests; >= 1 for increments in E2E).
- Response status codes and exact upstream targets.
- Deterministic route selection based on request predicates.
- Regex input limit tests use controlled large inputs (e.g., 5000-byte header value) and assert deterministic non-match.

**Deterministic Testing**:
- Unit tests: use mock counters to prove cost ordering (exact evaluation order).
- E2E tests: verify functionality (routing outcome, counter increments) without asserting execution order.
- Regex tests: use valid patterns + controlled inputs (within limit, exceeding limit) and assert deterministic results.

#### Tests

**Unit Tests** (core matcher logic):
1. Multi-method list: `methods: [GET, POST]` matches GET and POST, rejects PUT.
2. Multi-method list: empty list rejected by codec (ERR_INVALID_CONFIG).
3. Header prefix: `X-Tenant prefix "team-"` matches "team-alpha", rejects "user-alpha".
4. Header prefix: case-sensitive (matches "Team-alpha" if value is "Team-alpha", not "team-alpha").
5. Header regex: `X-Version regex "v[0-9]+"` matches "v123", rejects "vABC".
6. Header regex: input exceeds limit (mock input length > regex_input_max_bytes) → treated as non-match, counter increments.
7. Header present: `X-Debug present` matches any non-empty value, rejects missing header.
8. Header present: rejects empty string value.
9. Compound AND: `(method=GET AND header X-Tenant exact "alice")` matches both, rejects if either fails.
10. Compound OR: `(method=GET OR method=POST)` matches either.
11. Negation: `NOT (header X-Internal present)` matches if header absent.
12. **Cost ordering (unit test with mocks)**: verify regex evaluated after exact match using mock counters (exact evaluation order proven here).
13. Short-circuit AND: first predicate fails → second not evaluated (verify via mock counters).
14. Short-circuit OR: first predicate succeeds → second not evaluated (verify via mock counters).

**Integration Tests** (codec → core):
1. Parse multi-method list → `MethodMatcher::AnyOf(vec![Method::GET, Method::POST])`.
2. Parse header prefix → `HeaderMatcher::Prefix { name, prefix }`.
3. Parse header regex → `HeaderMatcher::Regex { name, pattern }` (pattern string, not compiled).
4. Parse header present → `HeaderMatcher::Present { name }`.
5. Parse regex pattern exceeding byte limit → `ERR_INVALID_CONFIG` with `field_path="routes[0].match.headers[\"x-custom\"].regex"`, `constraint="regex_pattern_too_long"`.
6. Parse regex pattern with invalid syntax → `ERR_INVALID_CONFIG` with `field_path="routes[0].match.headers[\"x-custom\"].regex"`, `constraint="regex_invalid_syntax"`.
7. Parse compound AND predicate → `PredicateNode::And(vec![...])`.
8. Parse compound OR predicate → `PredicateNode::Or(vec![...])`.
9. Parse negation → `PredicateNode::Not(Box::new(...))`.
10. Parse empty method list → `ERR_INVALID_CONFIG`.

**Integration Tests** (core/runtime → compiled regex):
1. Runtime compiles valid regex pattern with size limit → success, cached `Arc<Regex>`.
2. Runtime compiles regex pattern exceeding size limit → `ERR_INVALID_CONFIG` with `field_path="routes[0].match.headers[\"x-large\"].regex"`, `constraint="regex_size_limit_exceeded"`.

**E2E Tests** (full runtime):

Prefer integrating these verifications into existing routing E2E cases where feasible. If creating new cases, document invariant and rationale per E2E Case Integration policy.

1. **Multi-Method Routing** (case: `routing_multi_method`):
   - Route with `methods: [GET, POST]`.
   - GET request → matches route A.
   - POST request → matches route A.
   - PUT request → 404 (no match).
   - Assert: response status codes, upstream target.
   - Assert: `pavis_route_match_predicate_evaluations_total{operator="method"}` >= 1.

2. **Header Prefix Operator** (case: `routing_header_prefix`):
   - Route with `header X-Tenant prefix "team-"`.
   - Request with `X-Tenant: team-alpha` → matches.
   - Request with `X-Tenant: user-alpha` → 404.
   - Assert: response status codes, `pavis_route_match_predicate_evaluations_total{operator="prefix"}` >= 1.

3. **Header Regex Operator** (case: `routing_header_regex`):
   - Route with `header X-Version regex "v[0-9]+"`.
   - Request with `X-Version: v123` → matches.
   - Request with `X-Version: vABC` → 404.
   - Assert: response status codes, `pavis_route_match_predicate_evaluations_total{operator="regex"}` >= 1.

4. **Header Present Operator** (case: `routing_header_present`):
   - Route with `header X-Debug present`.
   - Request with `X-Debug: true` → matches.
   - Request with `X-Debug: ` (empty value) → 404.
   - Request with missing `X-Debug` → 404.
   - Assert: response status codes, `pavis_route_match_predicate_evaluations_total{operator="present"}` >= 1.

5. **Regex Input Length Limit** (case: `routing_regex_input_limit`):
   - Config: `regex_input_max_bytes = 1024`.
   - Route with `header X-Custom regex ".*"`.
   - Request with `X-Custom: <2048-byte value>` (exceeds limit) → 404 (treated as non-match).
   - Assert: `pavis_route_match_regex_input_too_large_total` >= 1.
   - Assert: response status code 404 (or next route if fallback exists).
   - Rationale: Verify input length limit enforcement, deterministic non-match.

6. **Compound OR Predicates** (case: `routing_compound_or`):
   - Route with `(path=/foo AND method=GET) OR (path=/bar AND method=POST)`.
   - GET /foo → matches.
   - POST /bar → matches.
   - GET /bar → 404.
   - POST /foo → 404.
   - Assert: response status codes, `pavis_route_match_predicate_evaluations_total{predicate_type="compound"}` >= 1.

7. **Negation Predicate** (case: `routing_negation`):
   - Route with `NOT (header X-Internal present)`.
   - Request with missing `X-Internal` → matches.
   - Request with `X-Internal: true` → 404.
   - Assert: response status codes.

8. **Cost Ordering Functionality** (extend existing case or create `routing_cost_ordering_functional`):
   - Route with multiple header predicates: regex, exact, prefix.
   - Request with all matching → route selected.
   - Assert: `pavis_route_match_predicate_evaluations_total` increments for all operators (>= 1 for each).
   - **Do NOT assert exact evaluation order in E2E** (proven in unit tests with mocks).

---

### 8) Route Retries/Timeouts Implementation (Full Policy)

**Goal**: wire full retry policy with per-try budgets, backoff, and idempotency constraints.

#### P0 vs P2 Boundary

**P0 (Baseline)** (current state):
- Global request timeout (single deadline for entire request, including all retries).
- No retries (fail on first upstream error).

**P2 (Full Policy)** (designed, not executed):
- Retry policy: max attempts, retryable reasons, backoff strategy, idempotency constraints.
- Per-try timeout: each attempt has independent deadline (within global budget).
- Backoff: fixed, linear, or exponential (configurable).
- Budget enforcement: global timeout encompasses all retries + backoff delays.

#### Contract

**Accepted Behavior**:

**Global Timeout vs Per-Try Timeout**:
- `request_timeout_ms`: outer bound for entire request lifecycle (initial attempt + all retries + backoff delays).
  - Field path: `routes[N].timeouts.request_timeout_ms` or `upstreams[N].timeouts.request_timeout_ms`.
  - Valid range: `1..=3600000` (1ms to 1 hour).
  - Enforced at: request start; deadline = `Instant::now() + request_timeout_ms`.
- `per_try_timeout_ms`: timeout for each individual upstream attempt.
  - Field path: `routes[N].timeouts.per_try_timeout_ms` or `upstreams[N].timeouts.per_try_timeout_ms`.
  - Valid range: `1..=request_timeout_ms`.
  - Enforced at: each attempt start; attempt deadline = `min(Instant::now() + per_try_timeout_ms, global_deadline)`.

**Connect and Read Timeouts**:
- `connect_timeout_ms`: timeout for TCP/TLS handshake (per attempt).
  - Field path: `upstreams[N].timeouts.connect_timeout_ms`.
  - Default: `5000` (5s).
  - On timeout: count as retryable failure (reason="connect_timeout").
- `read_timeout_ms`: timeout for reading response headers/body (per attempt).
  - Field path: `upstreams[N].timeouts.read_timeout_ms`.
  - Default: `30000` (30s).
  - On timeout: count as retryable failure (reason="read_timeout").
- Validation (codec): MUST reject `connect_timeout_ms > per_try_timeout_ms` with `ERR_INVALID_CONFIG` (constraint="connect_timeout_lte_per_try_timeout").
- Validation (codec): MUST reject `read_timeout_ms > per_try_timeout_ms` with `ERR_INVALID_CONFIG` (constraint="read_timeout_lte_per_try_timeout").
- Runtime behavior: actual timeout = `min(connect_timeout, per_try_timeout, remaining_global_budget)` for connect; `min(read_timeout, per_try_timeout, remaining_global_budget)` for read.

**Retryable Reasons**:
- Config: `retryable_reasons: ["status_code", "connect_timeout", "read_timeout", "per_try_timeout", "pool_full"]`.
- Config: `retryable_status_codes: [500, 502, 503, 504]` (applies when "status_code" is in retryable_reasons).
- Default retryable_reasons: `["status_code", "connect_timeout", "read_timeout"]`.
- Default retryable_status_codes: `[502, 503, 504]` (gateway errors; 500 opt-in).
- Reason enum (runtime): `RetryReason::StatusCode(u16)`, `RetryReason::ConnectTimeout`, `RetryReason::ReadTimeout`, `RetryReason::PerTryTimeout`, `RetryReason::PoolFull`, `RetryReason::ConnectError`.
- Retry logic: if failure reason is in retryable_reasons AND attempts remain AND budget allows → retry.
- **Schema Validation**: If `retryable_reasons` explicitly includes `"status_code"` AND `retryable_status_codes` is missing or empty → codec MUST reject with `ERR_INVALID_CONFIG` (field_path="routes[N].retry_policy.retryable_status_codes", constraint="required_when_status_code_retryable").
  - Rationale: prevent silent no-op retry configuration; if status_code retries are enabled, the user must explicitly list which codes.

**Max Attempts Bounds**:
- Config: `max_attempts: 3` (total attempts, including initial).
- Valid range: `1..=10`.
- Default: `1` (no retries, same as P0).
- Attempt counting: attempt 1 = initial; attempt 2+ = retries.

**Backoff Strategies**:
- Config: `backoff: { strategy: "exponential", base_ms: 100, max_ms: 5000 }`.
- Strategies:
  - `fixed`: constant delay = `base_ms`. Example: `backoff: { strategy: "fixed", base_ms: 200 }`.
  - `linear`: delay = `base_ms * (attempt - 1)`. Example: attempt 2 → 100ms, attempt 3 → 200ms.
  - `exponential`: delay = `base_ms * 2^(attempt - 2)`, capped at `max_ms`. Example: attempt 2 → 100ms, attempt 3 → 200ms, attempt 4 → 400ms, capped at 5000ms.
- Backoff delay: counted against global budget (sleep = min(calculated_delay, remaining_budget)).
- If remaining budget < backoff delay: skip backoff, attempt immediately if budget allows, else fail.

**Budget Enforcement**:
- Global deadline tracked throughout request lifecycle (stored in request context).
- Before each retry: check `remaining_budget = deadline.saturating_duration_since(Instant::now())`.
- If `remaining_budget == 0`: return 504 Gateway Timeout (ERR_REQUEST_TIMEOUT_GLOBAL), no more attempts.
- If `remaining_budget > 0` but < backoff delay: skip backoff, proceed immediately to next attempt (if attempts remain).
- Actual backoff sleep: `tokio::time::sleep(min(backoff_delay, remaining_budget))`.

**Idempotency Constraints**:
- Retries allowed ONLY for idempotent methods by default: GET, HEAD, OPTIONS, TRACE.
- Non-idempotent methods (POST, PUT, PATCH, DELETE): retries DISABLED by default.
- Override: config field `retry_non_idempotent: true` explicitly enables retries for non-idempotent methods.
  - Field path: `routes[N].retry_policy.retry_non_idempotent` or `upstreams[N].retry_policy.retry_non_idempotent`.
  - Default: `false`.
  - Warning: enabling for POST/PUT requires request body buffering (see below).

**Request Body Replayability**:
- **Buffered bodies**: if request body is fully buffered in memory, retry is safe (body can be replayed).
- **Streaming bodies**: if request body is streamed (chunked, large), body may not be replayable.
  - Detection: check `Transfer-Encoding: chunked` or if body is consumed without buffering.
- **Default behavior when body not replayable**:
  - If retry is required BUT body is not replayable: STOP retrying, return result of current/last attempt (do not convert to 500).
  - Emit counter: `pavis_request_body_not_replayable_total{upstream}`.
  - Emit log (warn level): "Request body not replayable for upstream 'X', retry aborted (attempt N, reason: R)".
- **Strict mode (optional)**: `fail_on_non_replayable_retry: true` (default: `false`).
  - If enabled AND retry required BUT body not replayable: return 500 Internal Server Error with `ERR_RETRY_BODY_NOT_REPLAYABLE`.
  - Field path: `routes[N].retry_policy.fail_on_non_replayable_retry`.
- **Buffering config**: `max_request_body_buffer_bytes: 1048576` (default 1MB; 0 = disable buffering).
  - Field path: `routes[N].retry_policy.max_request_body_buffer_bytes`.
  - Bodies <= this size are buffered in memory; larger bodies are streaming (not replayable unless client supports replay).

**Pool Interaction**:
- Each retry attempt is a new connection request, subject to `pool.max` limits and queue semantics (from P0 Item 2).
- If pool is full during retry attempt:
  - Request enters queue (if `pool.queue_capacity > 0`).
  - Queue wait time counts against per-try timeout (not global budget directly).
  - If queue wait exceeds `pool.queue_timeout_ms` OR per-try timeout: fail with reason="pool_full" (retryable if in retryable_reasons).
- "pool_full" reason: triggered when queue is full (queue_capacity reached) OR queue timeout exceeded.
- Retry logic: if "pool_full" is in retryable_reasons AND attempts remain AND budget allows → retry.
- Budget accounting: queue wait time is part of the attempt duration (counts against per-try timeout); backoff delay counts against global budget.

**Deterministic Behavior (No Adaptive)**:
- Retry count: fixed by config (`max_attempts`).
- Backoff strategy: fixed by config (no dynamic adjustment based on latency).
- Retryable reasons: fixed by config (no learning which reasons to retry).
- Frozen Data Plane: runtime reacts to config, emits metrics, but does NOT learn or adapt.

**Rejected Behavior**:
- Codec MUST reject `max_attempts = 0` with `ERR_INVALID_CONFIG` (field_path="routes[N].retry_policy.max_attempts", constraint="min_value=1").
- Codec MUST reject `max_attempts > 10` with `ERR_INVALID_CONFIG` (constraint="max_value=10").
- Codec MUST reject `per_try_timeout_ms > request_timeout_ms` with `ERR_INVALID_CONFIG` (constraint="per_try_timeout_lte_request_timeout").
- Codec MUST reject `connect_timeout_ms > per_try_timeout_ms` with `ERR_INVALID_CONFIG` (constraint="connect_timeout_lte_per_try_timeout").
- Codec MUST reject `read_timeout_ms > per_try_timeout_ms` with `ERR_INVALID_CONFIG` (constraint="read_timeout_lte_per_try_timeout").
- Codec MUST reject negative timeouts with `ERR_INVALID_CONFIG`.
- Codec MUST reject config where `"status_code"` is in `retryable_reasons` AND `retryable_status_codes` is missing or empty with `ERR_INVALID_CONFIG` (field_path="routes[N].retry_policy.retryable_status_codes", constraint="required_when_status_code_retryable").
- Runtime MUST NOT retry if idempotency constraint violated (unless `retry_non_idempotent = true`).
- Runtime MUST NOT learn or adapt retry behavior based on observed failure rates.

#### Ownership

| Layer   | Responsibility |
|---------|----------------|
| Codec   | Parse retry policy (`max_attempts`, `retryable_reasons`, `retryable_status_codes`, `backoff`, `retry_non_idempotent`, `fail_on_non_replayable_retry`, `max_request_body_buffer_bytes`); validate bounds (max_attempts 1..=10, timeouts positive, per_try <= request, connect/read <= per_try); validate `retryable_status_codes` is present when `status_code` is in `retryable_reasons`; materialize defaults; reject invalid configs with `ERR_INVALID_CONFIG` |
| Core    | Define retry policy model (`RetryPolicy`, `BackoffStrategy` enum, `RetryReason` enum, `IdempotencyConstraint`); document budget semantics and idempotency rules; provide budget tracking helpers (deadline calculation); define error codes (ERR_RETRY_EXHAUSTED, ERR_REQUEST_TIMEOUT_GLOBAL, ERR_RETRY_BODY_NOT_REPLAYABLE) |
| Runtime | Execute retry loop with deadline tracking; enforce per-try timeout; apply backoff (sleep within budget); respect idempotency constraints; check body replayability; emit metrics on retries/timeouts/backoff; return appropriate error codes; no learning or adaptation |

#### Implementation Notes

**Pingora Integration Points**:
- Retry loop: implement in `pingora_proxy::Session::upstream_request` callback or custom retry wrapper.
- Deadline tracking: store global deadline in request context (`ctx.deadline = Instant::now() + Duration::from_millis(request_timeout_ms)`).
- Per-try timeout: wrap each upstream call in `tokio::time::timeout_at(attempt_deadline, upstream_request)`.
- Connect/read timeouts: Pingora provides separate connect and read timeout settings; wire config values to Pingora peer configuration.
- Backoff: `tokio::time::sleep(min(backoff_delay, remaining_budget))` between attempts.

**Request Body Replay Safety**:
- **Buffering**: if body `Content-Length` <= `max_request_body_buffer_bytes`, read entire body into `Vec<u8>` before first attempt.
- **Replayability check**: before retry, verify body is buffered (`body_buffer: Option<Vec<u8>>`); if `None` (streaming) → handle per contract (stop retry or return 500 if strict mode).
- **Streaming detection**: check if `Transfer-Encoding: chunked` or `Content-Length` > buffer limit or unknown.

**Per-Try Deadline Enforcement**:
- Calculate: `attempt_deadline = min(Instant::now() + Duration::from_millis(per_try_timeout_ms), global_deadline)`.
- Wrap: `tokio::time::timeout_at(attempt_deadline, upstream_request)`.
- On timeout: log attempt timeout, increment `pavis_request_timeout_total{timeout_type="per_try"}`, check if retryable, retry if attempts remain.

**Backoff Implementation**:
- Fixed: `Duration::from_millis(base_ms)`.
- Linear: `Duration::from_millis(base_ms.saturating_mul((attempt - 1) as u64))`.
- Exponential: `Duration::from_millis(base_ms.saturating_mul(2u64.saturating_pow(attempt.saturating_sub(2) as u32)).min(max_ms))`.
- Sleep: `tokio::time::sleep(min(backoff_delay, remaining_budget))`.

**Controlled Time for Testing**:
- **Unit/integration tests**: use mock `Clock` abstraction (e.g., `tokio::time::pause()` and `tokio::time::advance()`) to control time deterministically without real sleeps.
- **E2E tests**: if mock time unavailable, use delays/timeouts that are unambiguously greater than configured values (e.g., upstream delay 2s > per_try_timeout 500ms) to ensure deterministic branching.

**Metrics Emission**:
- Increment counters on: retry attempt (with reason), retry exhausted, timeout (global/per-try/connect/read), backoff applied, body not replayable.
- Record histograms: backoff duration, attempt latency.

**Error Propagation**:
- Last attempt failure: return last upstream response or timeout error.
- Retry exhausted: return last upstream response, emit `ERR_RETRY_EXHAUSTED` in logs/metrics.
- Global timeout: return 504, emit `ERR_REQUEST_TIMEOUT_GLOBAL`.
- Body not replayable (strict mode): return 500, emit `ERR_RETRY_BODY_NOT_REPLAYABLE`.

#### Observability / Verdict Signals

**Metrics** (runtime):
- `pavis_request_retries_total{upstream, reason}`: counter per upstream and retry reason (reason="status_code", "connect_timeout", "read_timeout", "per_try_timeout", "pool_full").
- `pavis_request_retry_status_code_total{upstream, status}`: counter per upstream and retried status code (subset of above when reason="status_code").
- `pavis_request_retry_exhausted_total{upstream}`: counter when max attempts reached.
- `pavis_request_timeout_total{timeout_type}`: counter per timeout type (timeout_type="global", "per_try", "connect", "read").
- `pavis_request_backoff_duration_seconds{upstream, strategy}`: histogram of backoff delays (strategy="fixed", "linear", "exponential").
- `pavis_request_attempt_duration_seconds{upstream, attempt}`: histogram of per-attempt latency (attempt="1", "2", "3", ...).
- `pavis_request_body_not_replayable_total{upstream}`: counter when body not replayable and retry aborted.

**Logs** (debug level):
- "Retry attempt N/M for upstream 'X' (reason: status_code 503, backoff: 200ms, budget remaining: 1500ms)".
- "Retry attempt N/M for upstream 'X' (reason: connect_timeout, backoff: 100ms, budget remaining: 2800ms)".
- "Retry exhausted for upstream 'X' (max attempts: 3, last reason: status_code 502)".

**Logs** (warn level):
- "Request body not replayable for upstream 'X', retry aborted (attempt N, reason: status_code 503, streaming body)".
- "Global timeout exceeded for upstream 'X' (deadline exceeded, remaining attempts: N)".

**Error Codes**:
- `ERR_RETRY_EXHAUSTED`: max attempts reached, last response returned (logged, not returned as HTTP status).
- `ERR_REQUEST_TIMEOUT_GLOBAL`: global deadline exceeded (returned as 504).
- `ERR_RETRY_BODY_NOT_REPLAYABLE`: request body is streaming, retry required but not possible, strict mode enabled (returned as 500).

**Test Verdict**:
- Use failure injection (mock upstreams with controlled status/delay).
- Assert exact retry attempt counts (deterministic based on config).
- Assert budget exhaustion (deterministic based on controlled delays).
- Assert idempotency rules (POST not retried unless flag enabled).

#### Flake Avoidance Rule

**Prohibited Assertions**:
- Throughput thresholds (e.g., "at least X retries/sec").
- Latency percentiles (e.g., "p99 retry latency < Yms").
- Timing-sensitive assertions (e.g., "backoff completes within 50ms" — use mock time or controlled delays instead).
- Non-deterministic attempt counts (e.g., "2 or 3 attempts depending on timing").
- Histogram bucket value assertions in E2E tests (e.g., "histogram shows backoff durations 100ms, 200ms, 400ms" — move to unit tests with mock time).
- Success count assertions that depend on pool.max (e.g., "total successful requests <= pool.max" — pool.max limits concurrent usage, not success count).

**Required Assertions**:
- Exact retry attempt counts (based on config: max_attempts, retryable_reasons).
- Metric counter values (exact or deterministic bounds).
- Response status codes (200 vs 502 vs 504).
- Error codes for failure modes (ERR_RETRY_EXHAUSTED, ERR_REQUEST_TIMEOUT_GLOBAL).
- Budget exhaustion via controlled delays (e.g., upstream delay 2s > global timeout 1s → deterministic timeout).
- Pool.max invariant: assert `pavis_upstream_pool_size <= pool.max` at all times (gauge never exceeds limit), not success counts.

**Controlled Testing Strategy**:
- **Unit tests**: use `tokio::time::pause()` and `tokio::time::advance()` to control time without real sleeps; verify exact backoff durations and cap enforcement with mock clock.
- **E2E tests**: use mock upstreams with deterministic delays (e.g., upstream delay 2000ms > per_try_timeout 500ms) to ensure deterministic branching; assert retry occurred, reasons counters incremented, final outcome matches contract.
- Bounded test runtimes: set aggressive global timeouts to fail fast if test hangs.

#### Tests

**Unit Tests** (core retry logic):

Use mock `Clock` or `tokio::time::pause()` for deterministic time control.

1. Max attempts = 3, reason=status_code, status 503 → retries twice (total 3 attempts, deterministic).
2. Max attempts = 3, status 404 (non-retryable) → no retry (1 attempt).
3. Per-try timeout = 1s, upstream delays 2s (mock time) → timeout on attempt 1, retry triggered.
4. Global timeout = 2s, per-try timeout = 1s, backoff = 500ms, upstream delays 100ms (mock time) → attempt 1 (100ms) + backoff (500ms) + attempt 2 (100ms) + backoff (500ms) = 1200ms; attempt 3 starts at 1200ms, completes before global timeout 2s (deterministic).
5. Global timeout = 1s, backoff = 600ms (mock time) → attempt 1 (100ms) + backoff (600ms) = 700ms; attempt 2 starts at 700ms, but if fails, remaining budget 300ms < backoff 600ms → skip backoff, fail with global timeout (deterministic).
6. **Exponential backoff with mock time**: attempt 1 → 0ms, attempt 2 → 100ms, attempt 3 → 200ms, attempt 4 → 400ms (verify exact durations using mock clock).
7. **Backoff cap with mock time**: exponential backoff capped at max_ms; attempt 5 → 500ms (not 800ms); verify with mock clock.
8. Idempotency: GET request → retries allowed; POST request → retries disabled (unless flag enabled).
9. Request body buffered → retry allowed; body streaming → retry aborted (emit counter, return last result).
10. Connect timeout: upstream connect times out → reason=connect_timeout, retry triggered.
11. Budget exhausted during backoff (mock time): remaining budget 100ms < backoff 200ms → skip backoff, fail immediately if no budget for next attempt.

**Integration Tests** (codec → core):
1. Parse retry policy → `RetryPolicy { max_attempts: 3, retryable_reasons: [...], retryable_status_codes: [...], backoff: BackoffStrategy::Exponential { base_ms: 100, max_ms: 5000 }, retry_non_idempotent: false, fail_on_non_replayable_retry: false, max_request_body_buffer_bytes: 1048576 }`.
2. Parse `max_attempts = 0` → `ERR_INVALID_CONFIG` with `field_path="routes[0].retry_policy.max_attempts"`, `constraint="min_value=1"`.
3. Parse `max_attempts = 100` → `ERR_INVALID_CONFIG` with `constraint="max_value=10"`.
4. Parse `per_try_timeout_ms > request_timeout_ms` → `ERR_INVALID_CONFIG` with `constraint="per_try_timeout_lte_request_timeout"`.
5. Parse `connect_timeout_ms > per_try_timeout_ms` → `ERR_INVALID_CONFIG` with `constraint="connect_timeout_lte_per_try_timeout"`.
6. Parse `read_timeout_ms > per_try_timeout_ms` → `ERR_INVALID_CONFIG` with `constraint="read_timeout_lte_per_try_timeout"`.
7. Parse `backoff: { strategy: "exponential", base_ms: 100, max_ms: 5000 }` → `BackoffStrategy::Exponential { base_ms: 100, max_ms: 5000 }`.
8. Parse `retry_non_idempotent: true` → flag set in policy.
9. Parse `fail_on_non_replayable_retry: true` → flag set in policy.
10. **Parse `retryable_reasons: ["status_code"]` with missing `retryable_status_codes`** → `ERR_INVALID_CONFIG` with `field_path="routes[0].retry_policy.retryable_status_codes"`, `constraint="required_when_status_code_retryable"`.
11. **Parse `retryable_reasons: ["status_code"]` with empty `retryable_status_codes: []`** → `ERR_INVALID_CONFIG` with `field_path="routes[0].retry_policy.retryable_status_codes"`, `constraint="required_when_status_code_retryable"`.
12. Parse `retryable_reasons: ["connect_timeout"]` without `retryable_status_codes` → accepted (status_code not enabled).

**E2E Tests** (full runtime with failure injection):

Prefer integrating these verifications into existing upstream resilience E2E cases where feasible. If creating new cases, document invariant and rationale per E2E Case Integration policy.

1. **Retry Success** (case: `retry_success`):
   - Config: `max_attempts = 3`, `retryable_status_codes = [503]`, `per_try_timeout = 5s`, `backoff = { strategy: "exponential", base_ms: 100, max_ms: 5000 }`.
   - Mock upstream: attempt 1 → 503, attempt 2 → 503, attempt 3 → 200.
   - Assert: client receives 200 (final success).
   - Assert: `pavis_request_retries_total{reason="status_code"}` >= 2.
   - Assert: `pavis_request_retry_status_code_total{status="503"}` >= 2.
   - Assert: total attempts = 3 (deterministic, count via logs or attempt histogram).
   - Rationale: Verify retry success after transient failures.

2. **Retry Exhaustion** (case: `retry_exhausted`):
   - Config: `max_attempts = 3`, `retryable_status_codes = [502]`, `backoff = { strategy: "fixed", base_ms: 200 }`.
   - Mock upstream: all attempts → 502.
   - Assert: client receives 502 (last response).
   - Assert: `pavis_request_retry_exhausted_total` >= 1.
   - Assert: total attempts = 3 (deterministic, count via logs or attempt histogram).
   - Rationale: Verify retry exhaustion, last response returned.

3. **Non-Retryable Status** (case: `retry_non_retryable_status`):
   - Config: `max_attempts = 5`, `retryable_status_codes = [500]`.
   - Mock upstream: attempt 1 → 404.
   - Assert: client receives 404 immediately.
   - Assert: `pavis_request_retries_total` = 0 (no increments).
   - Assert: total attempts = 1 (deterministic).
   - Rationale: Verify non-retryable status codes fail fast.

4. **Global Timeout During Retry** (case: `retry_global_timeout`):
   - Config: `request_timeout = 1000ms`, `max_attempts = 5`, `per_try_timeout = 500ms`, `backoff = { strategy: "fixed", base_ms: 300 }`.
   - Mock upstream: all attempts → 503 (retryable) with deterministic delay 100ms.
   - Timeline (deterministic): attempt 1 (100ms) + backoff (300ms) + attempt 2 (100ms) + backoff (300ms) = 800ms. Attempt 3 starts at 800ms, completes at 900ms, backoff (300ms) would exceed global timeout 1000ms → attempt 4 not started, global timeout.
   - Assert: client receives 504 Gateway Timeout.
   - Assert: `pavis_request_timeout_total{timeout_type="global"}` >= 1.
   - Assert: total attempts = 3 (deterministic: attempts 1, 2, 3 complete; attempt 4 not started due to budget exhaustion).
   - Rationale: Verify global timeout enforcement during retry.

5. **Per-Try Timeout Enforced** (case: `retry_per_try_timeout`):
   - Config: `max_attempts = 3`, `per_try_timeout = 500ms`.
   - Mock upstream: all attempts delay 2000ms (exceeds per-try timeout deterministically).
   - Assert: each attempt times out at 500ms (deterministic).
   - Assert: `pavis_request_timeout_total{timeout_type="per_try"}` >= 3.
   - Assert: final response = 504 or last timeout error (after 3 attempts, deterministic).
   - Rationale: Verify per-try timeout enforcement, retry triggers.

6. **Backoff Occurred** (case: `retry_backoff_occurred`):
   - Config: `max_attempts = 3`, `backoff = { strategy: "exponential", base_ms: 100, max_ms: 500 }`.
   - Mock upstream: all attempts → 503.
   - Assert: `pavis_request_backoff_duration_seconds` histogram has >= 2 entries (backoff occurred between attempts).
   - **Do NOT assert exact bucket values or durations in E2E** (verify exact backoff timing with mock time in unit tests).
   - Rationale: Verify backoff mechanism is active (functional test, not timing precision).

7. **Idempotency Constraint** (case: `retry_idempotency`):
   - Config: `max_attempts = 3`, `retry_non_idempotent = false`.
   - Request: POST /api with body.
   - Mock upstream: attempt 1 → 503.
   - Assert: client receives 503 immediately (no retry, deterministic).
   - Assert: `pavis_request_retries_total` = 0.
   - Rationale: Verify idempotency constraint (POST not retried by default).

8. **Retry Non-Idempotent Enabled** (case: `retry_non_idempotent_enabled`):
   - Config: `max_attempts = 3`, `retry_non_idempotent = true`, `max_request_body_buffer_bytes = 1024`.
   - Request: POST /api with body < 1024 bytes (buffered).
   - Mock upstream: attempt 1 → 503, attempt 2 → 200.
   - Assert: client receives 200 (retry succeeded, deterministic).
   - Assert: `pavis_request_retries_total{reason="status_code"}` >= 1.
   - Rationale: Verify retry enabled for non-idempotent methods with buffered body.

9. **Request Body Not Replayable (Default Behavior)** (case: `retry_body_not_replayable_default`):
   - Config: `max_attempts = 3`, `retry_non_idempotent = true`, `fail_on_non_replayable_retry = false`, streaming body.
   - Request: POST /api with streaming body (chunked encoding or > buffer limit).
   - Mock upstream: attempt 1 → 503 (retryable).
   - Assert: client receives 503 (retry aborted, last result returned, deterministic).
   - Assert: `pavis_request_body_not_replayable_total` >= 1.
   - Assert: `pavis_request_retries_total` = 0 (no retry occurred).
   - Rationale: Verify streaming body prevents retry, returns last result (default behavior).

10. **Request Body Not Replayable (Strict Mode)** (case: `retry_body_not_replayable_strict`):
    - Config: `max_attempts = 3`, `retry_non_idempotent = true`, `fail_on_non_replayable_retry = true`, streaming body.
    - Request: POST /api with streaming body.
    - Mock upstream: attempt 1 → 503 (retryable).
    - Assert: client receives 500 Internal Server Error (deterministic).
    - Assert: error code in logs = ERR_RETRY_BODY_NOT_REPLAYABLE.
    - Assert: `pavis_request_body_not_replayable_total` >= 1.
    - Rationale: Verify strict mode converts non-replayable retry into explicit error.

11. **Connect Timeout Retry** (case: `retry_connect_timeout`):
    - Config: `max_attempts = 3`, `connect_timeout_ms = 500`, `retryable_reasons = ["connect_timeout"]`.
    - Mock upstream: connect times out (delay > 500ms, deterministic).
    - Assert: `pavis_request_retries_total{reason="connect_timeout"}` >= 1.
    - Assert: retry triggered (up to max_attempts, deterministic).
    - Rationale: Verify connect timeout counted as retryable.

12. **Interaction with pool.max** (extend existing pool test or create `retry_pool_interaction`):
    - Config: `pool.max = 2`, `pool.queue_capacity = 0`, `max_attempts = 3`, `retryable_reasons = ["pool_full"]`.
    - Mock upstream: all attempts → 503 (retryable).
    - Send 5 concurrent requests (exceeds pool.max).
    - Assert: `pavis_upstream_pool_size <= pool.max` at all sample points (gauge invariant, not success count).
    - Assert: `pavis_request_retries_total{reason="pool_full"}` >= 1 (some requests retry after pool rejection).
    - Assert: some requests receive 503 (pool full, deterministic based on pool.max).
    - **Do NOT assert "total successful requests <= pool.max"** (pool.max limits concurrent usage, not success count; requests may succeed after retry or queue wait).
    - Rationale: Verify retry interacts correctly with pool.max, pool_full is retryable if configured, pool.max invariant holds.
