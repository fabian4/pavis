# System Risk Remediation Plan

**Status**: In Progress
**Date**: 2026-01-14
**Source**: Comprehensive Audit Cycle (Jan 2026)
**Reference**: [Core Audit](../audit/core.audit.summary.md), [PVS Audit](../audit/pvs.audit.summary.md), [Runtime Audit](../audit/runtime.audit.summary.md), [E2E Audit](../audit/e2e.audit.summary.md)

## 1. Executive Summary

Following a comprehensive audit of the Pavis stack (`core`, `pvs`, `runtime`, `e2e`), the system has been certified as **Sound**. Remaining risks are concentrated in operational robustness and PVS limits, alongside two accepted low-severity items.

## 2. Risk Catalog & Triage

| ID | Component | Risk | Severity | Strategy | Status |
|---|---|---|---|---|---|
| **R-05** | PVS | **Payload Limit:** Hardcoded 100MB limit may block massive deployments. | **Medium** | **Config** | **Open** |
| **R-06** | E2E | **Environment Assumptions:** Reliance on host network/ports causes CI flakiness risks. | **Medium** | **Containerize** | **Open** |
| **R-07** | E2E | **Cert Lifecycle:** No tests for certificate rotation. | **Medium** | **Test** | **Open** |
| **R-09** | PVS | **Mmap Safety:** External modification of artifact file causes UB. | Low | **Accept** | **Accepted** |
| **R-10** | PVS | **Double Scan:** Checksum + Rkyv validation requires 2 passes. | Low | **Accept** | **Accepted** |

## 3. Remediation Strategy

### Phase 3: Operational Robustness (Backlog)
**Goal:** Improve operator experience and testing confidence.

#### [R-06] Containerized Test Runner (E2E)
- **Problem:** "Works on my machine" vs CI variance due to network/port assumptions.
- **Fix:** Define a `Dockerfile.test-runner` that encapsulates the test environment. Run E2E via `docker run`.
- **Outcome:** Deterministic test environment.
- **Status:** Open (confirmed: no containerized runner yet).

#### [R-07] Certificate Rotation Test (E2E)
- **Problem:** No validation that replacing cert files on disk works as expected without restart (or with reload).
- **Fix:** Add an `integrated` test case that regenerates certificates and triggers a reload.
- **Outcome:** Proven TLS operational lifecycle.
- **Status:** Open (confirmed: no cert rotation test yet).

#### [R-05] Configurable PVS Limits (PVS)
- **Problem:** 100MB limit is hardcoded.
- **Fix:** Allow build-time (`const fn`) or env-var override of `MAX_PAYLOAD_SIZE`.
- **Status:** Open (confirmed: hardcoded 100MB limit).

## 4. Accepted Risks

The following risks are accepted as inherent to the architectural choices:

- **[R-09] Mmap Safety (PVS):** Zero-copy performance requires `mmap`. The OS provides protection against most issues; malicious local modification is outside the threat model for the artifact loader.
- **[R-10] Double Scan (PVS):** Validating Checksum + Structure is necessary for safety. The linear cost `O(2N)` is the price of reliability.
- **[R-11] Mock Upstream Limits (E2E):** Real Nginx/Envoy upstreams would slow down tests significantly. `pavis-mock-upstream` is sufficient for functional logic.
