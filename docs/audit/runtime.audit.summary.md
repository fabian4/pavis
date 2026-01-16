Pavis Runtime Audit ? Actionable Recommendations (from Final Summary)

Executive intent
- Keep Frozen Data Plane boundary as-is (only .pvs, no YAML/JSON parsing).
- Raise production robustness by eliminating panic-on-input/hot-path panics and by hardening snapshot atomicity.
- Treat ?unsafe? as acceptable only with explicit safety contracts + guardrail tests.

1) Snapshot atomicity: make pinned snapshot a hard invariant
- Problem: Proxy::upstream_peer falls back to self.state.load() when ctx.runtime_state is None, allowing potential cross-snapshot mixing (route match on one snapshot, upstream select on another).
- Recommendation:
  - Enforce that ctx.runtime_state MUST be set for any request that reaches upstream selection.
  - If missing, return an InternalError (or explicit error) instead of silently falling back.
  - Add a focused test: simulate hot reload during a request and assert that route + upstream are resolved from the same snapshot version.
- Rationale: this is a correctness invariant (Atomic Switch / request handled by exactly one config version). Silent fallback is worse than a visible error.

2) Eliminate panic in request hot path: request-id time underflow
- Problem: generate_request_id uses duration_since(UNIX_EPOCH).unwrap(), which can panic under time rollback / misconfigured clock.
- Recommendation:
  - Replace unwrap with a safe fallback:
    - if SystemTime underflows, use 0, or a monotonic counter, or random seed + counter.
  - Log a warning once (rate-limited) if system clock is invalid.
- Rationale: this is per-request hot path; a single panic kills the process.

3) Convert ?external input? expect/unwrap to diagnosable errors (startup / service init)
- Problem: UpstreamResolver::new uses expect on env parsing and system DNS config reads.
- Recommendation:
  - Replace expect with Result-returning construction:
    - invalid PAVIS_DNS_SERVER => return an error with the invalid value and an example format
    - read_system_conf failure => return an error with guidance (permissions, container environment, etc.)
  - Choose a clear policy:
    - Strict mode: fail fast at startup (but as a clean error, not panic)
    - Lenient mode: disable DNS resolver service and continue, with loud logs/metrics
- Rationale: production should not ?panic on misconfig?; it should fail predictably and be diagnosable.

4) Lock poisoning strategy: remove unwraps on Mutex/RwLock in critical paths
- Problem: Mutex::lock().unwrap() and RwLock::read/write().unwrap() turn prior panics into repeated crashes or hard failures.
- Recommendation:
  - For callback registration/invocation and tracing reload layer:
    - handle poisoned locks by recovering the inner value (into_inner) or by disabling that optional subsystem (tracing/callback) while keeping proxy serving.
  - Decide explicitly: ?panic kills process? vs ?best-effort keep serving.? Prefer best-effort for sidecar runtime.
- Rationale: lock poisoning is a secondary failure; unwrap makes it catastrophic.

5) ?Worker started twice? is an internal invariant: keep or soften, but prove it
- Problem: AccessLogWorker::start_service expects single start and panics if started twice.
- Recommendation:
  - Option A (preferred): return Err("worker started twice") and no-op the second start.
  - Option B: keep expect, but add a wiring-level proof:
    - unit test or integration test ensuring the service is started exactly once under your server wiring.
- Rationale: this is less likely than env/time issues, but still avoidable.

6) Unsafe usage: require explicit safety contracts + guardrail tests
- Problem: unsafe blocks rely on external guarantees (pvs verify + layout invariants) and internal invariants (RequestId UTF-8, X509 shim layout).
- Recommendation:
  - For each unsafe block, add a ?SAFETY:? comment documenting:
    - what invariant is assumed
    - what enforces it (verify step, constructor, fixed layout type, etc.)
    - what would break it (version mismatch, struct layout change)
  - Add tests:
    - version mismatch must reject and never reach from_trusted
    - corrupted bytes must fail verify/load
    - RequestId always produces valid UTF-8 (or switch to a representation that doesn?t require unchecked UTF-8)
- Rationale: unsafe is acceptable when the contract is explicit and regression-protected.

7) Performance follow-ups (after correctness/panic hardening)
- Priority signals (when enabled):
  - metrics labels allocate per request (to_string for method/route/status/upstream)
  - tracing reload layer uses RwLock on every event
  - access log queue saturation / backpressure behavior
- Recommendation:
  - Run the proposed targeted benchmarks, but only after items (1)-(4) to avoid noisy restarts skewing results.
  - Produce a ?feature toggle cost matrix? (metrics on/off, tracing on/off, access log on/off) with p50/p99 and CPU.
- Rationale: you need quantified overhead by feature combination, not guesses.

8) Suggested execution order (minimal disruption)
- Step 1: enforce pinned snapshot invariant (remove fallback; add test)
- Step 2: remove request-id time unwrap panic (safe fallback; warn once)
- Step 3: replace DNS/env expects with Result + explicit policy (strict or lenient)
- Step 4: handle lock poisoning for optional subsystems (tracing/callback) without killing proxy
- Step 5: document unsafe ?SAFETY:? contracts + add guardrail tests
- Step 6: run Phase-5 benchmarks and record the toggle cost matrix

Acceptance criteria (what ?done? looks like)
- No panic in request hot path on malformed clocks or optional subsystem failures.
- No panic on external input (env/system conf); failures are structured and diagnosable.
- Snapshot atomicity is enforced: a request cannot mix config versions across route and upstream selection.
- Unsafe blocks have explicit safety contracts and regression tests protecting assumptions.
- Performance impact is measured per feature toggle combination (metrics/tracing/logging).
