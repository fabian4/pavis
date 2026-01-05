# Phase 4 Readiness: Execution Plan

This document tracks the completion of Phase 3.5 and required Technical Debt resolution before starting Phase 4 (xDS Integration).

## 1. Architecture Convergence (Phase 3.5)

### 1.1 Typed Pipeline Stages
**Goal:** Make invalid pipeline states unrepresentable.  
- [x] Define `ValidatedRuntimeConfig` in `pavis-core` as a newtype wrapper with private inner state.  
- [x] Implement `ValidatedRuntimeConfig::validate(RuntimeConfig) -> Result<Self>` constructor.  
- [x] Enforce the simplified codec contract in docs and APIs: `check → compile → materialize`, where `compile` returns `RuntimeConfig` and `materialize` is the only path to `ValidatedRuntimeConfig`.  
- [x] Ensure Relay only handles `ValidatedRuntimeConfig` or opaque `.pvs` blobs (no raw `RuntimeConfig` in state, pipeline, or distribution paths).  
- [x] **Doc/Task Contract Cleanup:** remove or replace all plan items that reference the old contract (e.g., “decode returns ValidatedRuntimeConfig”), and make this plan the single source of truth for the new contract wording.

### 1.2 Dependency Inversion
**Goal:** Decouple Relay from specific implementations.  
- [x] Refactor `pavis-relay` logic to remove direct dependencies on `pavis-ingest-file` and `pavis-codec-serde`.  
- [x] Update Relay's pipeline runner to work against `Box<dyn Ingest>` and `Box<dyn Codec>` traits.  
- [x] Move implementation-specific instantiation (e.g., `IngestFile::new`) to `main.rs` or `app.rs`.

### 1.3 Plugin-Style Composition
**Goal:** Enable modular builds.  
- [x] Add Cargo features to `pavis-relay` (e.g., `ingest-file`, `codec-serde`).  
- [x] Gate specific dependencies and factory logic behind feature flags.

### 1.4 Boundary Enforcement
**Goal:** Harden the "dumb pipe" invariant for the Relay.  
- [x] Remove all semantic config inspection from Relay: forbidden includes reading or branching on `RuntimeConfig` fields or any codec-specific DTOs; allowed includes `.pvs` header validation and checksum verification only.  
- [x] Add an **Opaque Artifact Proof Test** that demonstrates Relay can distribute valid artifacts it cannot decode/interpret (proof of opaque handling, not a semantic check).

## 2. Technical Debt Resolution

### 2.1 Testing & Quality (TD-1)
**Goal:** Bulletproof the core before xDS complexity.  
- [ ] Close unit testing gaps in `pavis-core` routing and validation logic.  
- [ ] Create `pavis-e2e/tests/chaos_reloads.rs` to verify state consistency under rapid-fire config churn.

### 2.2 Release Engineering (TD-2)
**Goal:** Guard the pipeline edge.  
- [ ] Implement strict content sniffing (magic byte check) in `pavis-ingest-file`.  
- [ ] Reject non-compliant files at the ingest layer before they reach the codec.

---

## Draft Execution Plan (Implementation Order)

### A. Typed Pipeline Stages
**Objective:** Enforce the canonical check → compile → materialize boundary.  
**Expected Outcome:** `compile` produces only `RuntimeConfig` and applies source defaults; `materialize` is the sole producer of `ValidatedRuntimeConfig` and the only caller of `pavis_core::validate_runtime`. Relay never handles raw `RuntimeConfig`.  
**DoD (checkbox items are required evidence):**  
- [x] `pavis-codec-api` docs and trait contract explicitly describe check → compile → materialize.  
- [x] A repo-wide audit removes legacy references to “decode returns ValidatedRuntimeConfig” or multi‑DTO pipeline wording.  
- [x] Relay interfaces and state accept only `ValidatedRuntimeConfig` or opaque `.pvs` bytes (no raw `RuntimeConfig` in Relay APIs).  
- [x] The plan itself is updated to be the single source of truth for the simplified contract (no contradictory items remain).

### B. Dependency Inversion
**Objective:** Keep relay generic and decoupled from concrete ingest/codec implementations.  
**Expected Outcome:** Relay depends only on ingest/codec traits, with concrete wiring at startup.  
**DoD:**  
- Relay pipeline is driven by `Box<dyn Ingest>` and `Box<dyn Codec>`.  
- Concrete instantiation moved to `main.rs`/`app.rs`.  
- Relay core modules have no direct imports of `pavis-ingest-file` or `pavis-codec-serde`.

### C. Plugin-Style Composition
**Objective:** Make relay build composition explicit and minimal without adding new abstraction layers.  
**Expected Outcome:** Feature flags gate concrete ingest/codec dependencies while preserving default behavior.  
**DoD:**  
- Feature flags added for `ingest-file` and `codec-serde`.  
- Default feature set preserves current relay behavior.  
- Feature-disabled builds compile cleanly with unused wiring removed.

### D. Boundary Enforcement
**Objective:** Preserve the relay as a dumb pipe for opaque artifacts.  
**Expected Outcome:** Relay never inspects or branches on runtime config semantics; only artifact integrity is handled.  
**DoD (checkbox items are required evidence):**  
- [x] Relay code contains no reads or pattern-matching of `RuntimeConfig` or codec DTOs.  
- [x] **Opaque Artifact Proof Test** exists and passes, showing Relay distributes artifacts it cannot decode.  

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
- `pavis-ingest-file` validates magic bytes/format.  
- Tests cover truncated and malformed file rejection.
