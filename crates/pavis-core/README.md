# Pavis Core

## 1. Crate Overview
`pavis-core` provides the fundamental primitives, data structures, and validation logic for the Pavis data plane. It defines the canonical representation of the system's state, used by both the control plane (for generation and validation) and the data plane (for execution).

Its primary responsibilities are:
- Defining the `RuntimeConfig` root object and its constituent types.
- Implementing semantic validation via `validate_runtime` to ensure configuration integrity.
- Providing binary serialization support via `rkyv` for zero-copy-capable artifact distribution.
- Offering optional `serde` support for human-readable format interop.

It explicitly does not handle:
- Network I/O or proxy execution (delegated to `pavis`).
- File-level persistence or CLI interactions (delegated to `pavis-pvs` and `pavctl`).
- Codec-specific logic for external formats (delegated to `pavis-codec-*`).

## 2. Features
- **Zero-Copy Readiness**: Utilizes `rkyv` for all core configuration types, enabling efficient binary serialization and high-performance loading.
- **Semantic Validation Engine**: Implements deep checks for invariants, including:
  - Upstream connectivity and destination existence.
  - Path normalization (RFC compliance).
  - Regex complexity and validity.
  - HTTP header name and value character set compliance.
  - Listener and TLS configuration integrity.
- **Strong Typing**: Employs newtype wrappers (e.g., `UpstreamName`, `Path`, `Weight`) to prevent logic errors and enforce domain constraints (like `NonZeroU16` for weights).
- **Flexible Serialization**: Supports `serde` via an optional feature flag for seamless integration with YAML/JSON tools.

## 3. Module Breakdown

### `runtime`
The authoritative source for all data plane types.
- `server`: Listener and TLS configurations.
- `routing`: Virtual host and path matching definitions.
- `upstream`: Backend cluster, endpoint, and load balancer types.
- `telemetry`: Logging, metrics, and tracing specifications.
- `headers`: Rules for request and response header manipulation.
- `types`: Core primitives like `Duration`, `Weight`, and various identifiers.

### `validate`
The logic layer responsible for enforcing system invariants.
- `validate_runtime`: The entry point for checking a `RuntimeConfig`.
- Sub-modules (`server`, `routes`, `upstreams`, `headers`) handle domain-specific semantic checks.
- Defines `CoreValidationError` to provide precise diagnostic information.

## 4. Public API Surface

### `RuntimeConfig`
The root structure representing the complete desired state of a Pavis instance.
Builders are available to construct configs without relying on struct literals:
- `RuntimeConfigBuilder`
- `ListenerBuilder`
- `UpstreamBuilder`

### `ValidatedRuntimeConfig`
A wrapper type that guarantees the inner `RuntimeConfig` has passed all semantic checks.
- `unsafe fn from_trusted(config)`: Wraps a config without re-checking; caller must uphold validation.
- `into_inner()`: Unwraps back to the raw config.

### `validate_runtime`
`pub fn validate_runtime(config: RuntimeConfig) -> CoreValidationResult<ValidatedRuntimeConfig>`
The primary function for verifying configuration integrity.

### `CoreValidationError`
An enum documenting all possible semantic violations, ranging from `DuplicateUpstream` to `PathNotNormalized`.

## 5. Configuration and Runtime Behavior

### Semantic Invariants
The implementation enforces the following at validation time:
- **Normalization**: Paths for Prefix and Exact matches must start with `/` and contain no trailing slashes (except for `/` itself).
- **Referential Integrity**: All destinations in routes must point to defined upstream clusters.
- **TLS Consistency**: If TLS is enabled, certificate and key paths must be non-empty.
- **TLS SNI Safety**: `verify=full` with `sni=auto` requires DNS endpoints or a route host rewrite.
- **Port Uniqueness**: Listener, admin, and metrics ports must not conflict.
- **Regex Safety**: Regular expressions are limited to 2048 characters and must be valid per the `regex` crate.

### Feature Flags
- `serde`: Enables `Serialize` and `Deserialize` derives on core types.

## 6. Error Handling and Invariants

### Validation Errors
The crate provides detailed error context, such as identifying the specific host and route where a regex failed or which upstream is missing.

### Invariants
- **Non-Zero Constraints**: Weights and attempt counts are enforced to be non-zero at the type level where possible, and via validation otherwise.
- **Character Sets**: Header names and values are checked against HTTP specification limits (no control characters, valid separators).

## 7. Non-Goals and Explicit Limitations
- **External Format Parsing**: This crate does not parse YAML/JSON; it expects types already mapped to its structures.
- **Hardware-Specific Optimizations**: The core types are architecture-agnostic.
- **Runtime Performance Tuning**: While it provides the structures (like worker counts), the core crate does not implement the thread-pooling or scheduling logic.
