# Pavis Codec API

## 1. Crate Overview
`pavis-codec-api` defines the trait and contract for converting opaque configuration artifacts (like YAML files, Kubernetes CRDs, or xDS snapshots) into validated, runtime-ready `RuntimeConfig` objects. It serves as the architectural boundary between the "Ingest" layer (getting bytes/objects) and the "Core" layer (execution semantics).

Its primary responsibilities are:
- Defining the `Codec` trait, which enforces a linear compilation pipeline.
- Establishing the `materialize` method as the single authoritative entry point for creating validated configurations.
- Providing standardized error types (`CodecError`) that wrap core validation failures.
- Defining compaction levels (`CompactionLevel`) for optimizing configuration size.

It explicitly does not handle:
- Specific format implementations (e.g., `serde` logic is in `pavis-codec-serde`).
- I/O or file watching (delegated to ingest crates).
- Execution logic (delegated to `pavis`).

## 2. Features
- **Enforced Pipeline**: The `Codec` trait mandates a strict `check -> compile -> compact -> validate` sequence, preventing the creation of invalid runtime configurations.
- **Stateful Parsing**: Supports attaching ephemeral state (via `CheckedArtifact`) during the check phase, allowing parsers to reuse intermediate results (like a parsed AST) during compilation.
- **Compaction Support**: Includes a hook for semantics-preserving optimizations (e.g., deduplicating shared structures) before final validation.
- **Type-Safe Errors**: `CodecError` ensures that core semantic validation errors are preserved and propagated without loss of context.

## 3. Module Breakdown

### `lib.rs`
Contains the entire API definition.
- `Codec` trait: The core interface for all configuration adapters.
- `CodecError`: The unified error type.
- `CompactionLevel`: Enum controlling optimization aggressiveness.
- `CheckedArtifact`: A wrapper around the raw artifact that proves it has passed the `check` phase.

## 4. Public API Surface

### `Codec` Trait
The interface that all specific format adapters must implement.
- `check(&self, art: Artifact) -> Result<CheckedArtifact, Self::Error>`: Validates framing and format.
- `compile(&self, checked: &CheckedArtifact) -> Result<RuntimeConfig, Self::Error>`: Transforms the source into the canonical runtime model, applying defaults.
- `compact(&self, cfg: &mut RuntimeConfig, level: CompactionLevel)`: Optional optimization step.
- `materialize(&self, art: Artifact, level: CompactionLevel) -> Result<ValidatedRuntimeConfig, Self::Error>`: The sealed method that orchestrates the full pipeline.

### `CheckedArtifact`
A state container that proves an artifact has been inspected. It allows storing an `Arc<dyn Any>` to pass parsed state (like a `serde_json::Value`) from `check` to `compile` to avoid re-parsing.

## 5. Configuration and Runtime Behavior

### Pipeline Invariants
The `materialize` method enforces strict architectural rules:
1. **Semantic Defaults**: Must be applied within `compile`. The runtime core does not assume defaults.
2. **Canonical Validation**: Must happen exactly once, at the very end of the pipeline, via `pavis_core::validate_runtime`.
3. **Forward-Only**: The pipeline is designed to go from Source -> Runtime. Reverse engineering (Runtime -> Source) is not a requirement of this trait.

## 6. Error Handling and Invariants

### Error Propagation
All implementations must define an `Error` type that implements `From<CoreValidationError>`. This ensures that if the final validation step fails, the specific semantic error (e.g., "Duplicate Upstream Name") is returned to the user, not a generic "compilation failed" error.

### Thread Safety
The `Codec` trait requires `Send + Sync` on both the implementer and the error type, ensuring that compilation can be offloaded to worker threads in an async runtime.

## 7. Non-Goals and Explicit Limitations
- **Reverse Codecs**: This crate defines the Source-to-Runtime path only.
- **Parsing Logic**: This crate defines *interfaces*. Actual parsing (JSON/YAML/Proto) happens in implementation crates.
- **Validation Logic**: Canonical validation rules are defined in `pavis-core`, not here. This crate merely invokes them.
