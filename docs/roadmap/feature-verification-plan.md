# Feature Verification Implementation Plan (P0/P1/P2)

This plan expands the P0/P1/P2 items into concrete engineering steps and test
expectations. It does not change behavior by itself.

## P0 – Safety & Correctness

### 1) Header/Method Routing Gap

Goal: matcher supports method/header predicates in addition to path/host.

Steps:
1. Locate matcher evaluation in runtime routing (crates/pavis) and matcher types
   in pavis-core. Identify where host/path are evaluated.
2. Extend codec DTOs to parse method/header selectors and map into new core
   matcher structures. Use explicit enums (no Option for toggles).
3. Extend core matcher model to represent method/header predicates. Ensure
   defaults are materialized in codec.
4. Update runtime route selection to apply method/header predicates alongside
   existing host/path checks, preserving current evaluation order.
5. Add unit tests covering: path+method, path+header, host+path+method+header,
   and negative cases (non-matching method/header).
6. Add E2E test with two routes sharing path but differing method/header, prove
   correct route selection and traffic outcome.

### 2) Upstream pool.max Ignored

Goal: enforce connection caps from config.

Steps:
1. Trace pool.max from codec -> core -> runtime to confirm where it is dropped.
2. Wire pool.max into Pingora upstream pool configuration for both HTTP and TLS
   paths (if split).
3. Add validation for invalid caps (e.g., zero or negative) in codec/core with
   explicit error message.
4. Add integration test that sets a low pool.max and validates capped
   concurrency under load (metrics/logs/observed behavior).
5. Add regression test that higher pool.max allows increased concurrency.

### 3) Inbound mTLS (rustls) Blocked

Goal: reject unsupported configs when rustls is selected.

Steps:
1. Add validation gate: rustls + inbound mTLS config -> reject with explicit
   error and message.
2. Ensure runtime never attempts to apply rustls inbound mTLS settings.
3. Add E2E test: rustls + inbound mTLS config is rejected with exact message.
4. Add E2E test: OpenSSL backend accepts inbound mTLS config (if supported).

### 4) Outbound Custom CA (rustls) Blocked

Goal: reject unsupported per-peer CA bundles when rustls is selected.

Steps:
1. Add validation gate: rustls + per-peer CA bundle -> reject with explicit
   error and message.
2. Add E2E test: rustls config rejected with exact message.
3. Add E2E test: OpenSSL backend accepts per-peer CA bundle (if supported).

## P1 – Process & Test Hardening

### 5) Backend-aware E2E Table

Goal: CI publishes a backend matrix (rustls/OpenSSL) of Supported/Rejected/Skipped.

Steps:
1. Define the canonical list of checks and backends to report.
2. Add a generator script or test that emits a machine-readable artifact
   (CSV/JSON) and a Markdown table.
3. Integrate into CI to publish the table and fail on regressions
   (e.g., Supported -> Rejected/Skipped).
4. Store outputs under a consistent path (tests/output or bench/output).

### 6) Validation Suite for Ignored Fields

Goal: configs that are parsed but ignored/blocked fail fast with precise errors.

Steps:
1. Inventory fields currently parsed but ignored/blocked.
2. Add an E2E suite that submits these configs and asserts rejection.
3. Enforce stable, precise error messages for each rejected config.

## P2 – Feature Candidates

### 7) Header/Method Routing Enhancements

Goal: expand matcher expressiveness with host+path+method+header logic.

Steps:
1. Extend matcher model to support compound predicates with explicit enums.
2. Add codec support for expanded predicates and default materialization.
3. Add runtime implementation and E2E coverage.
4. If required, gate under a feature flag or GA release switch.

### 8) Route Retries/Timeouts Implementation (Full Policy)

Goal: wire full retry policy with per-try budgets and backoff.

Steps:
1. Define retry policy model (retryable statuses, backoff, per-try timeout).
2. Wire into Pingora request lifecycle with correct deadline handling.
3. Add integration tests covering retry success, retry exhaustion, and per-try
   timeout enforcement.
