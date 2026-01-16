# Execution Plan: Runtime Hardening Improvements

Scope: Implement actionable recommendations from `docs/audit/runtime.audit.summary.md` for `crates/pavis` without changing core architecture (Frozen Data Plane, .pvs-only runtime).

Status Legend: [ ] pending, [x] complete, [-] not applicable.

1) Snapshot atomicity (hard invariant)
- [x] Remove fallback to `self.state.load()` in `Proxy::upstream_peer` when `ctx.runtime_state` is missing.
- [x] Return a structured InternalError with request_id + route/upstream context when the snapshot is missing.
- [x] Add focused test: pin one RuntimeState in context while `self.state` points to another; assert a diagnosable error.

2) Request ID robustness
- [x] Replace `SystemTime::duration_since(UNIX_EPOCH).unwrap()` with a safe fallback on time underflow.
- [x] Add warning-once (rate-limited) log for invalid system clock.
- [x] Add tests for non-panicking behavior and RequestId UTF-8 validity.

3) Resolver init: diagnosable errors
- [x] Replace `expect/unwrap` in `UpstreamResolver::new` with Result-returning logic.
- [x] Choose strict vs lenient policy and implement explicitly.
- [x] Add tests for invalid `PAVIS_DNS_SERVER` and system DNS config failure via injection/mocking.

4) Lock poisoning handling
- [x] Replace `Mutex/RwLock` unwraps in critical paths with recover-or-disable behavior.
- [x] Add tests that poison the lock and confirm the proxy continues operating.

5) Unsafe contracts + guardrails
- [x] Add `// SAFETY:` comments for each unsafe block (invariant, enforcement, violation).
- [x] Add guardrail tests for version mismatch, corrupted PVS rejection, and RequestId UTF-8 assumptions.

6) Benchmark follow-up (after steps 1–4)
- [ ] Run Phase-5 targeted benchmarks.
- [ ] Produce feature-toggle cost matrix (metrics/tracing/access log) with p50/p99 + RPS + reload impact.

Verification
- [x] Run `make ci-local` after Rust code changes.

Out-of-scope
- [-] `doc/CODE_REVIEW.md` is deprecated; no action required.

## TODO (Audit-Driven Improvements)
- [x] Remove request-path panics and snapshot fallback in `crates/pavis`.
- [x] Replace panic-on-lock/enum paths in `crates/pavis-core`, `crates/pavis-codec-serde`, and `crates/pavis-testkit` with explicit errors.
- [x] Add size guards for unbounded reads in `crates/pavis-ingest-file` and `crates/pavis-relay`.
- [x] Document unsafe assumptions and add guardrail tests for request ID UTF-8 and config validation paths.
- [ ] Reduce skipped E2E cases and stabilize timing-sensitive tests.

### E2E stabilization plan (for TODO above)
- [ ] Inventory all skipped E2E cases and classify by cause (unsupported feature, planned feature, unclear spec, infra-only).
- [ ] For unsupported-feature skips (e.g., rustls per-peer CA limits), decide: keep as blocked with a tracking issue, or replace with a supported coverage case.
- [ ] For planned-feature skips (timeout/retry), either implement feature per roadmap or move test to a future plan with explicit gating.
- [ ] For unclear-spec skips (e.g., LKG rejection semantics), align expected behavior with core/runtime policy and update the test accordingly.
- [ ] Replace fixed sleeps in timing-sensitive cases with polling helpers (add `wait_for_*` helpers as needed).
- [ ] Re-run targeted E2E suites to confirm reduced skips and stabilized timing.
