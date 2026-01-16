# Execution Plan: Runtime Hardening Improvements

Scope: Implement actionable recommendations from `docs/audit/runtime.audit.summary.md` for `crates/pavis` without changing core architecture (Frozen Data Plane, .pvs-only runtime).

Status Legend: [ ] pending, [x] complete, [-] not applicable.

1) Snapshot atomicity (hard invariant)
- [ ] Remove fallback to `self.state.load()` in `Proxy::upstream_peer` when `ctx.runtime_state` is missing.
- [ ] Return a structured InternalError with request_id + route/upstream context when the snapshot is missing.
- [ ] Add focused test: pin one RuntimeState in context while `self.state` points to another; assert a diagnosable error.

2) Request ID robustness
- [ ] Replace `SystemTime::duration_since(UNIX_EPOCH).unwrap()` with a safe fallback on time underflow.
- [ ] Add warning-once (rate-limited) log for invalid system clock.
- [ ] Add tests for non-panicking behavior and RequestId UTF-8 validity.

3) Resolver init: diagnosable errors
- [ ] Replace `expect/unwrap` in `UpstreamResolver::new` with Result-returning logic.
- [ ] Choose strict vs lenient policy and implement explicitly.
- [ ] Add tests for invalid `PAVIS_DNS_SERVER` and system DNS config failure via injection/mocking.

4) Lock poisoning handling
- [ ] Replace `Mutex/RwLock` unwraps in critical paths with recover-or-disable behavior.
- [ ] Add tests that poison the lock and confirm the proxy continues operating.

5) Unsafe contracts + guardrails
- [ ] Add `// SAFETY:` comments for each unsafe block (invariant, enforcement, violation).
- [ ] Add guardrail tests for version mismatch, corrupted PVS rejection, and RequestId UTF-8 assumptions.

6) Benchmark follow-up (after steps 1–4)
- [ ] Run Phase-5 targeted benchmarks.
- [ ] Produce feature-toggle cost matrix (metrics/tracing/access log) with p50/p99 + RPS + reload impact.

Verification
- [ ] Run `make ci-local` after Rust code changes.

Out-of-scope
- [-] `doc/CODE_REVIEW.md` is deprecated; no action required.

## TODO (Audit-Driven Improvements)
- Remove request-path panics and snapshot fallback in `crates/pavis`.
- Replace panic-on-lock/enum paths in `crates/pavis-core`, `crates/pavis-codec-serde`, and `crates/pavis-testkit` with explicit errors.
- Add size guards for unbounded reads in `crates/pavis-ingest-file` and `crates/pavis-relay`.
- Document unsafe assumptions and add guardrail tests for request ID UTF-8 and config validation paths.
- Reduce skipped E2E cases and stabilize timing-sensitive tests.
