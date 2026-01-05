# Runtime Configuration Reference

> **Status:** Reference
> **Role:** Canonical definition of the fully materialized runtime configuration and its YAML form.

This document describes the `pavis_core::RuntimeConfig` structure consumed by the Pavis runtime. YAML is a codec-level representation emitted or accepted by source-specific codecs and is not the canonical model.

# Configuration Translation and Defaults

This section defines the configuration translation pipeline, DTO stages, and defaulting boundaries for Pavis. It applies to YAML, xDS, and CRD sources.

## Configuration Translation Pipeline

Pavis configuration MUST pass through the following stages in order:
- Source Config → Source DTO
- Source DTO → Partial Pavis DTO
- Partial Pavis DTO → Structurally Complete Pavis DTO
- Structurally Complete Pavis DTO → RuntimeConfig
- RuntimeConfig → core semantic validation

Source configurations are expected to be sparse. Sparse input is allowed and encouraged; it is not an error at the source layer.

Pipeline responsibility mapping (mandatory):
- Source Config → Source DTO: Implemented by the source-specific codec input layer. Ingest layers MUST NOT apply defaults or semantics.
- Source DTO → Partial Pavis DTO: Implemented by the source-specific codec. codec-api MUST NOT apply semantics here.
- Partial Pavis DTO → Structurally Complete Pavis DTO: Implemented by codec-api structural completion utilities, invoked by the source-specific codec.
- Structurally Complete Pavis DTO → RuntimeConfig: Implemented by the source-specific codec, including source-specific semantic defaults.
- RuntimeConfig → core semantic validation: Implemented by core validation; runtime and relay MUST NOT compensate for missing intent.

## DTO Stages and Intent

### Source DTO
- Represents the source format directly.
- MAY be sparse and incomplete.
- MUST preserve source-specific fields and semantics.
- MUST NOT include runtime-specific assumptions.

### Partial Pavis DTO
- Represents a normalized, source-agnostic shape.
- MAY still be sparse and incomplete.
- MUST remove source quirks and normalize field naming.
- MUST NOT be semantically defaulted.

### Structurally Complete Pavis DTO
- Represents a complete shape with all required containers and fields present.
- MUST eliminate structural absence via empty containers and explicit disabled states.
- MUST NOT introduce semantic defaults beyond structural completion.
- Exists to provide a stable, source-agnostic boundary for validation and conversion.
- Is mandatory for all codecs, even if the source appears complete.

### RuntimeConfig
- Represents the fully materialized runtime configuration.
- MUST be fully specified with all semantic defaults applied.
- MUST be suitable for core semantic validation without additional inference.
- Is semantically final and immutable in meaning; no later layer may infer, compensate, or repair intent.

## Defaults: Structural vs Semantic

### Structural Completion
Structural completion is about shape only:
- Options are resolved to explicit empty or disabled states.
- Containers are normalized to consistent presence.
- Shape consistency is guaranteed across all fields.

Structural completion MAY occur in codec-api utilities and in concrete codecs. It MUST NOT introduce or imply runtime semantics.

### Semantic Defaults
Semantic defaults include, but are not limited to:
- Timeouts
- Retry policies
- Protocol choices
- Policy defaults

Semantic defaults MUST be applied only by source-specific codecs. Runtime, relay, and core layers MUST NOT apply semantic defaults.

## Codec API Responsibilities

codec-api MAY:
- Define the transformation phases and their ordering.
- Provide structural completion utilities.
- Enforce that pipeline steps are executed in sequence.

codec-api MUST NOT:
- Define default values for semantic fields.
- Embed source-specific semantics.
- Override or replace concrete codec behavior.
- Perform semantic inference of any kind.
- Apply “obvious” or “universal” defaults.
- Derive values based on intent or meaning rather than structure.

## Concrete Codec Responsibilities

Concrete codecs MUST:
- Apply source-specific semantic defaults.
- Normalize source quirks into the partial Pavis DTO.
- Derive any runtime-ready fields required to produce RuntimeConfig.

Concrete codecs MUST NOT:
- Share defaults across codecs.
- Delegate semantic defaulting to runtime, relay, or core.

## Non-Goals and Forbidden Patterns

The following are forbidden:
- Shared default tables across codecs.
- Semantic defaulting in runtime, relay, or core.
- Semantic decisions in codec-api or other API layers.
- Semantic or policy-based compensation in relay.
- “Safety defaults” or fallback behavior in relay.

## Invariants

The following rules are mandatory for code review:
- Source configs MUST be accepted as sparse inputs.
- Source DTOs MUST preserve source semantics and MUST NOT embed runtime assumptions.
- Partial Pavis DTOs MUST be source-agnostic and MUST NOT include semantic defaults.
- Structurally Complete Pavis DTOs MUST be shape-complete and MUST NOT apply semantic defaults.
- RuntimeConfig MUST be fully specified before core validation.
- Core validation MUST treat RuntimeConfig as semantically complete.
- codec-api MUST NOT define or apply semantic defaults.
- Concrete codecs MUST apply semantic defaults that are scoped to their source.

# RuntimeConfig (Rust)

The canonical Rust schema lives in these files:
- `crates/pavis-core/src/runtime/mod.rs`
- `crates/pavis-core/src/runtime/types.rs`
- `crates/pavis-core/src/runtime/server.rs`
- `crates/pavis-core/src/runtime/telemetry.rs`
- `crates/pavis-core/src/runtime/upstream.rs`
- `crates/pavis-core/src/runtime/routing.rs`
- `crates/pavis-core/src/runtime/headers.rs`

## Normative Semantics
- `HeadersPolicy::Disabled` means no header mutations are applied.
- Regex compilation happens at runtime load/swap and is not stored in the schema.

# YAML Reference

The canonical annotated YAML template lives at:
- `examples/config-template.yaml`

Notes:
- YAML durations (`idle`, `connect`, `timeout`, `per_try`) accept human-friendly strings and are materialized into milliseconds in `RuntimeConfig`.
- Endpoint weights and destination weights are `NonZeroU16` in the runtime config.
