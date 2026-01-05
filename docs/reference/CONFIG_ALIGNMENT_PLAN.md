# Configuration Alignment Execution Plan

This plan aligns the implementation with the current architecture and configuration specification. It excludes migration work and focuses only on code alignment and enforcement.

## Short-term (alignment & safety)

### Step 1: Make pipeline stages explicit in code
- Goal: Model Source DTO → Partial Pavis DTO → Structurally Complete Pavis DTO → RuntimeConfig as explicit types or wrappers.
- Scope:
  - `crates/pavis-codec-api/src/lib.rs`
  - `crates/pavis-codec-serde/src/lib.rs`
  - `crates/pavis-codec-serde/src/config/mod.rs`
- Concrete tasks:
  - Introduce stage marker types or newtype wrappers for each stage.
  - Update codec interfaces to pass through these stages explicitly (no direct Source DTO → RuntimeConfig).
  - Keep external behavior unchanged.
- Acceptance checks:
  - Code paths show explicit stage transitions.
  - No direct conversion from Source DTO to RuntimeConfig in codec implementations.
- Risk: Medium
- Status: Completed

### Step 2: Remove semantic defaults from Source DTO parsing
- Goal: Ensure Source DTOs remain sparse and contain no semantic defaults.
- Scope:
  - `crates/pavis-codec-serde/src/config/types/server.rs`
  - `crates/pavis-codec-serde/src/config/types/telemetry.rs`
  - `crates/pavis-codec-serde/src/config/types/upstreams.rs`
  - `crates/pavis-codec-serde/src/config/types/routes.rs`
- Concrete tasks:
  - Remove `#[serde(default)]` that injects semantic defaults (matcher, pool timeouts, access log, TLS enabled, etc.).
  - Keep optional fields optional; avoid populating with semantic values during deserialization.
- Acceptance checks:
  - Source DTO parsing yields sparse values when fields are omitted.
  - Defaults are not applied in serde layer.
- Risk: Medium
- Status: Completed

### Step 3: Isolate structural completion as a dedicated step
- Goal: Separate structural completion from semantic defaults.
- Scope:
  - `crates/pavis-codec-api/src/lib.rs`
  - `crates/pavis-codec-serde/src/config/convert.rs`
- Concrete tasks:
  - Add a structural completion function that normalizes containers and explicit disabled states.
  - Ensure this step happens after Partial Pavis DTO and before semantic defaults.
- Acceptance checks:
  - Structural completion produces a shape-complete DTO without semantic values.
  - Semantic defaults are applied only after structural completion.
- Risk: Low
- Status: Completed

## Medium-term (structural clarity)

### Step 4: Constrain codec-api to structural-only operations
- Goal: Prevent semantic inference or defaults in codec-api.
- Scope:
  - `crates/pavis-codec-api/src/lib.rs`
- Concrete tasks:
  - Replace or narrow `compact` to a clearly structural-only operation.
  - Add API docs and tests that disallow semantic defaulting at codec-api level.
- Acceptance checks:
  - codec-api exports only structural utilities.
  - Tests fail if semantic defaults are applied in codec-api.
- Risk: Low
- Status: Completed

### Step 5: Enforce RuntimeConfig finality at runtime boundaries
- Goal: Ensure runtime/relay never infer or compensate for missing intent.
- Scope:
  - `crates/pavis-core/src/runtime.rs`
  - `crates/pavis/src/load.rs`
  - `crates/pavis-relay/src/pipeline.rs`
- Concrete tasks:
  - Restrict `ValidatedRuntimeConfig::from_trusted` usage to controlled boundaries.
  - Require validated artifacts in runtime entry points.
  - Add assertions or checks to prevent late semantic changes.
- Acceptance checks:
  - Runtime entry points only accept validated configs.
  - No post-validation mutation or inference of config fields.
- Risk: Medium
- Status: Completed

## Long-term (governor-readiness)

### Step 6: Harden relay as an opaque artifact pipeline
- Goal: Make relay strictly transport and validation orchestration.
- Scope:
  - `crates/pavis-relay/src/pipeline.rs`
  - `crates/pavis-relay/src/handlers.rs`
- Concrete tasks:
  - Ensure relay never inspects semantic fields or applies any policy decisions.
  - Add guardrails in relay to prevent semantic compensation or fallbacks.
- Acceptance checks:
  - Relay code does not branch on semantic values.
  - Relay tests assert configs are treated as opaque validated artifacts.
- Risk: Low
- Status: Completed

## Final Alignment Guarantees

After completion:
- All pipeline stages are explicit and enforceable in code.
- Structural completion is isolated from semantic defaults.
- Semantic defaults are applied only by source-specific codecs.
- RuntimeConfig is treated as final and immutable in meaning.
- Relay remains an opaque artifact transport without semantic behavior.
