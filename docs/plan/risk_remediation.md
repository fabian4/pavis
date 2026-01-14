# System Risk Remediation Plan

**Status**: Draft
**Date**: 2026-01-14
**Source**: Comprehensive Audit Cycle (Jan 2026)
**Reference**: [Core Audit](../audit/core.audit.summary.md), [PVS Audit](../audit/pvs.audit.summary.md), [Runtime Audit](../audit/runtime.audit.summary.md), [E2E Audit](../audit/e2e.audit.summary.md)

## 1. Executive Summary

Following a comprehensive audit of the Pavis stack (`core`, `pvs`, `runtime`, `e2e`), the system has been certified as **Sound**. However, several specific risks were identified across architectural, operational, and performance dimensions. This plan outlines the strategy to remediate critical flaws and manage accepted risks.

## 2. Risk Catalog & Triage

| ID | Component | Risk | Severity | Strategy |
|---|---|---|---|---|
| **R-01** | Runtime | **Reload Consistency:** Request phases (filter vs peer) may see different config snapshots during hot reload. | **Critical** | **Fix** |
| **R-02** | Core | **Ambiguous Validation:** `assume_validated` vs `from_trusted` allows safe bypass of validation logic. | **High** | **Refactor** |
| **R-03** | Core | **API Rigidity:** Public struct fields prevent non-breaking additions (missing `#[non_exhaustive]` on structs). | **High** | **Refactor** |
| **R-04** | Runtime | **Request ID Allocation:** `String` allocation in ultra-hot path. | **Medium** | **Optimize** |
| **R-05** | PVS | **Payload Limit:** Hardcoded 100MB limit may block massive deployments. | **Medium** | **Config** |
| **R-06** | E2E | **Environment Assumptions:** Reliance on host network/ports causes CI flakiness risks. | **Medium** | **Containerize** |
| **R-07** | E2E | **Cert Lifecycle:** No tests for certificate rotation. | **Medium** | **Test** |
| **R-08** | Core | **Regex Overhead:** Compiling regexes in validation loop. | **Medium** | **Optimize** |
| **R-09** | PVS | **Mmap Safety:** External modification of artifact file causes UB. | Low | **Accept** |
| **R-10** | PVS | **Double Scan:** Checksum + Rkyv validation requires 2 passes. | Low | **Accept** |

## 3. Remediation Strategy

### Phase 1: Critical Reliability (Immediate)
**Goal:** Ensure correctness guarantees are never violated and safety contracts are clear.

#### [R-01] Snapshot Pinning (Runtime)
- **Problem:** `Proxy` fetches `state.load()` twice per request. If a reload happens between filter and upstream selection, the upstream might vanish.
- **Fix:** Update `RouterContext` to hold an `Arc<RuntimeState>`. Capture it once in `request_filter` and pass it to `upstream_peer`.
- **Outcome:** Atomic request handling with respect to configuration.

#### [R-02] Validation Contract Hardening (Core)
- **Problem:** `assume_validated` is safe but bypasses checks, confusing the "witness" guarantee of `ValidatedRuntimeConfig`.
- **Fix:** Deprecate `assume_validated`. Force usage of `unsafe fn from_trusted` for trusted paths, or `validate_runtime` for untrusted.
- **Outcome:** Type system strictly enforces validation provenance.

### Phase 2: API & Performance (Next Release)
**Goal:** Future-proof the API and optimize hot paths before v1.0 lock-in.

#### [R-03] Builder Pattern Adoption (Core)
- **Problem:** Adding fields to `RuntimeConfig` breaks downstream builds using struct literals.
- **Fix:** Implement `Builder` pattern for `RuntimeConfig`, `Listener`, `Upstream`. Mark structs `#[non_exhaustive]`.
- **Outcome:** Stable API surface allowing non-breaking field additions.

#### [R-04] Request ID Optimization (Runtime)
- **Problem:** `format!` allocates a new `String` for every request.
- **Fix:** Use a thread-local formatter or a fixed-size stack buffer (e.g., `compact_str`, `ulid`, or `uuid`) to eliminate heap allocation.
- **Outcome:** Reduced heap churn and GC pressure.

#### [R-08] Regex Compilation Optimization (Core)
- **Problem:** Validation compiles all regexes, potentially slow for large configs.
- **Fix:** Use `lazy_static` or a compilation cache if this becomes a bottleneck, or accept as one-time load cost.
- **Outcome:** Faster validation.

### Phase 3: Operational Robustness (Backlog)
**Goal:** Improve operator experience and testing confidence.

#### [R-06] Containerized Test Runner (E2E)
- **Problem:** "Works on my machine" vs CI variance due to network/port assumptions.
- **Fix:** Define a `Dockerfile.test-runner` that encapsulates the test environment. Run E2E via `docker run`.
- **Outcome:** Deterministic test environment.

#### [R-07] Certificate Rotation Test (E2E)
- **Problem:** No validation that replacing cert files on disk works as expected without restart (or with reload).
- **Fix:** Add an `integrated` test case that regenerates certificates and triggers a reload.
- **Outcome:** Proven TLS operational lifecycle.

#### [R-05] Configurable PVS Limits (PVS)
- **Problem:** 100MB limit is hardcoded.
- **Fix:** Allow build-time (`const fn`) or env-var override of `MAX_PAYLOAD_SIZE`.

## 4. Accepted Risks

The following risks are accepted as inherent to the architectural choices:

- **[R-09] Mmap Safety (PVS):** Zero-copy performance requires `mmap`. The OS provides protection against most issues; malicious local modification is outside the threat model for the artifact loader.
- **[R-10] Double Scan (PVS):** Validating Checksum + Structure is necessary for safety. The linear cost `O(2N)` is the price of reliability.
- **[R-11] Mock Upstream Limits (E2E):** Real Nginx/Envoy upstreams would slow down tests significantly. `pavis-mock-upstream` is sufficient for functional logic.
