# Pavis Codec Serde

## 1. Crate Overview
`pavis-codec-serde` is the primary implementation of the `Codec` trait for human-readable configuration formats. It enables the Pavis ecosystem to ingest configurations written in standard YAML and JSON, serving as the bridge between developer-friendly text files and the optimized binary runtime format.

Its primary responsibilities are:
- Implementing the `SerdeCodec` to parse YAML and JSON into intermediate DTOs.
- Defining `SerdeConfig` and related types that mirror the core configuration but with relaxed types (e.g., `Option<T>`, `String`) to support partial inputs and defaults.
- Applying semantic defaults (normalization) during the compilation phase, ensuring that omitted fields (like timeouts or load balancer strategies) are resolved to safe defaults before reaching the core validation layer.
- Providing conversion logic to transform these DTOs into `pavis_core::RuntimeConfig` types.

It explicitly does not handle:
- Core semantic validation (e.g., checking if an upstream exists). This is delegated to `pavis-core` via the `Codec` pipeline.
- Binary serialization (delegated to `pavis-pvs`).

## 2. Features
- **Multi-Format Support**: Natively handles both YAML and JSON inputs via `serde_yaml` and `serde_json`.
- **Default Application**: Automatically populates missing fields with sensible defaults (e.g., RoundRobin load balancing, 60s idle timeouts) during conversion.
- **Human-Friendly Types**: Uses `humantime-serde` to allow specifying durations as strings (e.g., "5s", "100ms") instead of raw milliseconds.
- **Bi-Directional Conversion**: Supports converting `RuntimeConfig` back into `SerdeConfig`, enabling tools like `pavctl convert` to reverse-engineer binary configs into readable text.

## 3. Module Breakdown

### `config`
Contains the data transfer objects (DTOs) and conversion logic.
- `types`: Defines the `SerdeConfig` struct and all nested types (`Listener`, `Upstream`, `VirtualHost`) annotated with `serde` attributes.
- `convert`: Implements `TryFrom` and `From` traits to map between `SerdeConfig` and `pavis_core::RuntimeConfig`, handling type parsing (IPs, socket addresses) and default application.
- `validation`: Performs structural validation specific to the serde format (e.g., checking string formats before conversion).

### `serde_helpers`
Utility functions for uniform parsing of different formats and error handling during deserialization.

### `lib.rs`
Implements the `Codec` trait for `SerdeCodec`, wiring the parsing, compilation, and validation steps into the standard pipeline defined by `pavis-codec-api`.

## 4. Public API Surface

### `SerdeCodec`
The main struct implementing the `Codec` trait.
- `check`: Verifies the input format matches the expected type (JSON/YAML).
- `compile`: Parses the bytes into `SerdeConfig`, validates structure, applies defaults, and converts to `RuntimeConfig`.

### `SerdeConfig`
The top-level DTO.
- `parse_str` / `parse_bytes`: Helpers to load configuration from memory.
- `build()`: Converts the DTO into a `RuntimeConfig`, triggering the internal conversion pipeline.

### `SerdeFormat`
Enum specifying supported input formats (`Yaml`, `Json`).

## 5. Configuration and Runtime Behavior

### Defaults
This crate defines the source-of-truth for defaults when using text configuration:
- **Load Balancer**: RoundRobin.
- **Protocol**: HTTP/1.1.
- **Timeouts**: Connect (5s), Idle (60s).
- **Workers**: Auto (matches CPU cores).

### Parsing Rules
- **Durations**: Must be parsable by `humantime` (e.g., "1h", "300ms").
- **Addresses**: Listen addresses and endpoint IPs are parsed using `std::net` parsers. Invalid formats cause compilation errors.

## 6. Error Handling and Invariants

### Compilation Errors
Errors during parsing (syntax) or conversion (invalid values) are wrapped in `CodecError::Compile`. This differentiates them from core semantic errors.

### Round-Trip Safety
The crate guarantees that a valid `RuntimeConfig` can be serialized back to `SerdeConfig` without data loss, though defaults may be made explicit in the output.

## 7. Non-Goals and Explicit Limitations
- **Preserving Comments**: `serde` does not preserve YAML/JSON comments, so converting back to text will lose them.
- **Schema Validation**: While it validates structure, it does not currently use a formal schema validator (like JSON Schema) before parsing, relying instead on strong typing and custom validation logic.
