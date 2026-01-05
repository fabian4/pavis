# Phase 4 Readiness: Execution Plan

This document tracks the completion of Phase 3.5 and required Technical Debt resolution before starting Phase 4 (xDS Integration).

## 2. Technical Debt Resolution

### 2.1 Testing & Quality (TD-1)
**Goal:** Bulletproof the core before xDS complexity.  
- [ ] Close unit testing gaps in `pavis-core` routing and validation logic.  
- [ ] Create `pavis-e2e/tests/chaos_reloads.rs` to verify state consistency under rapid-fire config churn.

### 2.2 Release Engineering (TD-2)
**Goal:** Guard the pipeline edge.  
- Note: magic byte validation remains in `pavis-pvs`; `pavis-ingest-file` should not duplicate it.  
- [x] Reject non-compliant files at the ingest layer before they reach the codec.

---

## Draft Execution Plan (Implementation Order)

### E. Testing & Quality
**Objective:** Reduce correctness risk before Phase 4 without expanding scope.  
**Expected Outcome:** Core routing/validation edges are covered and reload churn is exercised end-to-end.  
**DoD:**  
- `pavis-core` tests cover routing precedence and regex/validation edge cases.  
- `pavis-e2e/tests/chaos_reloads.rs` verifies convergence under rapid updates.

### F. Release Engineering
**Objective:** Block invalid artifacts as early as possible at ingest boundaries.  
**Expected Outcome:** File ingest rejects non-compliant content before codec work begins.  
**DoD:**  
- Note: magic byte validation remains in `pavis-pvs`; `pavis-ingest-file` should not duplicate it.  
- `pavis-ingest-file` rejects non-compliant files before codec processing.  
- Tests cover empty/whitespace files, unsupported formats, and malformed UTF-8 rejection.
