# pavis-pvs

The PVS protocol implementation for Pavis configuration artifacts.

## Purpose

`pavis-pvs` implements the binary protocol boundary for `.pvs` configuration files, handling serialization, deserialization, and integrity verification. It provides the gatekeeper functionality ensuring artifacts are tamper-evident and runtime-compatible.

## Responsibilities

- Reading and writing `.pvs` binary artifacts
- Verifying artifact integrity (magic bytes, version, checksum)
- Validating `rkyv` archive layouts
- Providing zero-copy access to validated configurations

## Module Structure

- **`header`**: PVS header structure and constants
- **`read`**: Low-level header reading with bounds checking
- **`write`**: Artifact serialization and creation
- **`verify`**: Integrity verification pipeline
- **`error`**: Error types and categorization

## Public API

```rust
// Write a configuration artifact
pavis_pvs::write("config.pvs", &runtime_config)?;

// Load and verify
let config = pavis_pvs::load("config.pvs")?;

// Verify only (no deserialization)
pavis_pvs::verify_file("config.pvs")?;
```

## Related Documentation

- **Binary Format Specification**: See [`docs/specs/pvs-format.md`](../../docs/specs/pvs-format.md)
- **Architecture**: See [`/ARCHITECTURE.md`](../../ARCHITECTURE.md)
