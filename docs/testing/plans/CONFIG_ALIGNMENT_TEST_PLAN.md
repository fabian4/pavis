# Configuration Alignment Test Plan

This test plan defines the verification strategy for the [Configuration Alignment Execution Plan](../../reference/CONFIG_ALIGNMENT_PLAN.md).
It ensures that code changes strictly adhere to the architectural invariants regarding pipeline stages, default handling, and runtime safety.

## Short-term (Alignment & Safety)

### Step 1: Explicit Pipeline Stages

**Goal:** Verify that the code strictly enforces the transition: `SourceArtifact` → `CheckedArtifact` → `RuntimeConfig` → `ValidatedRuntimeConfig`.

| Test ID | Type | Description | Acceptance Criteria |
| :--- | :--- | :--- | :--- |
| `T1.1` | Unit (Type) | **Verify Type Isolation**<br>Attempt to pass an `Artifact` directly into `compile` or obtain `ValidatedRuntimeConfig` without `materialize`. | **Compilation Error** or explicit type mismatch. `compile` accepts `CheckedArtifact`, and `materialize` is the only producer of validated configs. |
| `T1.2` | Integration | **Pipeline Flow Verification**<br>Trace a config object through the `pavis-codec-serde` pipeline. | The object must pass through `check` → `compile` → `materialize` in order. |

**Target Implementation:**
- New tests in `crates/pavis-codec-serde/src/lib.rs` or `tests/pipeline.rs`.

### Step 2: Remove Semantic Defaults from Parsing

**Goal:** Ensure source deserialization yields a sparse object (all `Option::None`) rather than injecting values like "5s" or "true".

| Test ID | Type | Description | Acceptance Criteria |
| :--- | :--- | :--- | :--- |
| `T2.1` | Unit | **Sparse Deserialization (Server)**<br>Deserialize `{}` (empty YAML/JSON) into the serde config type. | All fields (port, bind, etc.) must be `None`. `#[serde(default)]` must not inject values. |
| `T2.2` | Unit | **Sparse Deserialization (Routes)**<br>Deserialize a route with minimal fields. | Optional fields like `timeout` or `retry_policy` must be `None`. |
| `T2.3` | Unit | **Sparse Deserialization (Upstream)**<br>Deserialize an upstream cluster with minimal fields. | `connect_timeout`, `lb_policy` etc. must be `None`. |

**Target Implementation:**
- Modify/Add tests in `crates/pavis-codec-serde/src/config/types/*.rs`.

### Step 3: Isolate Structural Completion

**Goal:** Verify that structural completion lives in concrete codecs (not codec-api), initializes containers (Vecs, Maps) and explicit "Disabled" enums, and **does not** inject semantic policy defaults.

| Test ID | Type | Description | Acceptance Criteria |
| :--- | :--- | :--- | :--- |
| `T3.1` | Unit | **Structural vs. Semantic Separation**<br>Pass a sparse DTO through the structural completion step. | - `Option<Vec<T>>` becomes `Vec<T>` (empty).<br>- `Option<Enum>` (structural) becomes `Enum::Disabled`.<br>- **Semantic fields** (e.g., `timeout: Option<Duration>`) remain `None`. |

**Target Implementation:**
- New test module in `crates/pavis-codec-serde/src/config/convert.rs`.

## Medium-term (Structural Clarity)

### Step 4: Constrain Codec-API

**Goal:** Ensure `pavis-codec-api` only enforces the pipeline boundary and core validation, with no semantic logic.

| Test ID | Type | Description | Acceptance Criteria |
| :--- | :--- | :--- | :--- |
| `T4.1` | Unit | **API Purity Check**<br>Review/Test `pavis-codec-api` helpers. | Helpers must only enforce ordering and validation; no semantic defaults or source-specific behavior. |

### Step 5: Enforce RuntimeConfig Finality

**Goal:** Ensure the Runtime and Relay layers treat `ValidatedRuntimeConfig` as immutable and do not perform "late" defaulting.

| Test ID | Type | Description | Acceptance Criteria |
| :--- | :--- | :--- | :--- |
| `T5.1` | Integration | **Runtime Rejection of Partial Config**<br>Attempt to initialize `pavis-core` Runtime with a config missing mandatory fields (bypass validation if possible to test safety). | Runtime constructor must fail or `ValidatedRuntimeConfig` constructor must be the *only* entry point and panic/fail on invalid input. |
| `T5.2` | Unit | **Immutability Check**<br>Verify `RuntimeConfig` fields are not `pub mut` or accessible for mutation after creation. | Field access is read-only. No setters exist on `RuntimeConfig`. |

**Target Implementation:**
- `crates/pavis-core/src/runtime.rs` tests.

## Long-term (Governor-readiness)

### Step 6: Harden Relay Opaque Handling

**Goal:** Relay must act as a dumb pipe.

| Test ID | Type | Description | Acceptance Criteria |
| :--- | :--- | :--- | :--- |
| `T6.1` | Integration | **Relay Content Agnosticism**<br>Pass a syntactically valid but semantically "nonsense" config (e.g., weird ports, valid but useless routes) through Relay. | Relay accepts, versions, and distributes the artifact without error. It does not validate business rules. |
| `T6.2` | Unit | **No Inspection**<br>Code audit/test to ensure Relay does not deserialize the inner `RuntimeConfig` to check fields. | Relay operates on `Blob` or `Artifact` abstractions, not `RuntimeConfig`. |

**Target Implementation:**
- `crates/pavis-relay/tests/pipeline.rs`.
