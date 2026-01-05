# Reference

> **Role:** Canonical Developer Reference for APIs and Configuration.

## 1. Runtime Configuration

This document describes the `pavis_core::RuntimeConfig` structure consumed by the Pavis runtime. YAML is a codec-level representation emitted or accepted by source-specific codecs and is not the canonical model.

### 1.1 Configuration Translation and Defaults

This section defines the configuration translation pipeline and defaulting boundaries for Pavis. It applies to YAML, xDS, and CRD sources.

#### Configuration Translation Pipeline

Pavis configuration MUST pass through the following stages in order:
- SourceArtifact → CheckedArtifact (codec `check`)
- CheckedArtifact → RuntimeConfig (codec `compile`)
- RuntimeConfig → core semantic validation (codec `materialize`)

Source configurations are expected to be sparse. Sparse input is allowed and encouraged; it is not an error at the source layer.

Pipeline responsibility mapping (mandatory):
- SourceArtifact → CheckedArtifact: Implemented by codec `check`. Ingest layers MUST NOT apply defaults or semantics.
- CheckedArtifact → RuntimeConfig: Implemented by codec `compile`, including parsing, normalization, structural completion, and source-specific semantic defaults.
- RuntimeConfig → core semantic validation: Implemented by codec `materialize` via `pavis-core`; runtime and relay MUST NOT compensate for missing intent.

#### Codec-Internal DTOs (Optional)

**Source DTO (codec-internal)**
- Represents the source format directly.
- MAY be sparse and incomplete.
- MUST preserve source-specific fields and semantics.
- MUST NOT include runtime-specific assumptions.

**Structurally Complete DTO (codec-internal)**
- Represents a complete shape with all required containers and fields present.
- MUST eliminate structural absence via empty containers and explicit disabled states.
- MUST NOT introduce semantic defaults beyond structural completion.
- Exists to provide a stable, codec-local boundary for conversion.

**RuntimeConfig**
- Represents the fully materialized runtime configuration.
- MUST be fully specified with all semantic defaults applied.
- MUST be suitable for core semantic validation without additional inference.
- Is semantically final and immutable in meaning; no later layer may infer, compensate, or repair intent.

#### Defaults: Structural vs Semantic

**Structural Completion**
Structural completion is about shape only:
- Options are resolved to explicit empty or disabled states.
- Containers are normalized to consistent presence.
- Shape consistency is guaranteed across all fields.

Structural completion occurs inside concrete codecs (typically during `compile`). It MUST NOT introduce or imply runtime semantics.

**Semantic Defaults**
Semantic defaults include, but are not limited to:
- Timeouts
- Retry policies
- Protocol choices
- Policy defaults

Semantic defaults MUST be applied only by source-specific codecs. Runtime, relay, and core layers MUST NOT apply semantic defaults.

#### Codec API Responsibilities

codec-api MAY:
- Define the check/compile/materialize ordering.
- Provide the `CheckedArtifact` carrier and `CompactionLevel`.
- Enforce that core validation runs exactly once in `materialize`.

codec-api MUST NOT:
- Define default values for semantic fields.
- Embed source-specific semantics.
- Override or replace concrete codec behavior.
- Perform semantic inference of any kind.
- Apply “obvious” or “universal” defaults.
- Provide or require structural completion utilities.
- Require or enforce codec-internal DTO stages.

#### Concrete Codec Responsibilities

Concrete codecs MUST:
- Apply source-specific semantic defaults.
- Perform structural completion before building `RuntimeConfig`.
- Derive any runtime-ready fields required to produce RuntimeConfig.

Concrete codecs MUST NOT:
- Share defaults across codecs.
- Delegate semantic defaulting to runtime, relay, or core.

#### Non-Goals and Forbidden Patterns

The following are forbidden:
- Shared default tables across codecs.
- Semantic defaulting in runtime, relay, or core.
- Semantic decisions in codec-api or other API layers.
- Semantic or policy-based compensation in relay.
- “Safety defaults” or fallback behavior in relay.

#### Invariants

The following rules are mandatory for code review:
- Source configs MUST be accepted as sparse inputs.
- `compile` MUST parse, normalize, structurally complete, and apply semantic defaults.
- RuntimeConfig MUST be fully specified before core validation.
- Core validation MUST treat RuntimeConfig as semantically complete.
- codec-api MUST NOT define or apply semantic defaults.
- Concrete codecs MUST apply semantic defaults that are scoped to their source.

### 1.2 Normative Semantics
- `HeadersPolicy::Disabled` means no header mutations are applied.
- Regex compilation happens at runtime load/swap and is not stored in the schema.
- **Durations**: YAML durations (`idle`, `connect`, `timeout`, `per_try`) accept human-friendly strings and are materialized into milliseconds in `RuntimeConfig`.
- **Weights**: Endpoint weights and destination weights are `NonZeroU16` in the runtime config.

### 1.3 RuntimeConfig (Rust)

The canonical Rust schema lives in these files:
- `crates/pavis-core/src/runtime/mod.rs`
- `crates/pavis-core/src/runtime/types.rs`
- `crates/pavis-core/src/runtime/server.rs`
- `crates/pavis-core/src/runtime/telemetry.rs`
- `crates/pavis-core/src/runtime/upstream.rs`
- `crates/pavis-core/src/runtime/routing.rs`
- `crates/pavis-core/src/runtime/headers.rs`

---

## 2. Relay HTTP API Reference

The canonical definition of the Pavis Relay HTTP API.

### 2.1 Endpoints

#### `GET /v1/config`

Fetches the latest configuration artifact. Supports Long-Polling.

**Request Headers:**

| Header | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `X-Pavis-Artifact-Version` | `u64` | Yes | The version currently held by the client. |

**Query Parameters:**

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `wait_ms` | `u64` | `1000` | Max time to hold connection if up-to-date (Max 10s). |

**Responses:**

| Status | Description |
| :--- | :--- |
| `200 OK` | New configuration available. Body is `.pvs` binary. |
| `204 No Content` | Timeout reached, no new config. Client should retry. |
| `400 Bad Request` | Missing or invalid headers/params. |

**Response Headers (Lineage & Traceability):**

| Header | Type | Description |
| :--- | :--- | :--- |
| `X-Pavis-Artifact-Version` | `u64` | The version of the returned artifact. |
| `X-Pavis-Generated-At` | `String` | RFC3339 timestamp of when the artifact was generated. |

#### `GET /v1/status`

Operational status and health. Returns internal state (name, active version, checksum, uptime).

### 2.2 Protocol Details

The Relay ensures configuration propagation via HTTP Long-Polling. See `docs/SPECIFICATIONS.md` for the server-side state machine and long-polling logic.
