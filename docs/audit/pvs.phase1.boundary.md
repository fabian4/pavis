Audit phase name: Phase 1: Boundary & Responsibility Audit
Target crate: crates/pavis-pvs
Generation timestamp: 2026-01-07T10:35:00Z
AI model identifier: unknown

# Phase 1: Boundary & Responsibility Audit

## 1. Boundary & Responsibility Verification

The `crates/pavis-pvs` crate strictly adheres to its mandate as a binary container handler for the Pavis system.

- **No Business Semantics**: The crate does not inspect the contents of `RuntimeConfig`. It treats the configuration payload as a opaque byte blob during serialization/deserialization and checksum verification.
- **No Defaults or Policy**: There is no evidence of policy decisions or default population (e.g., filling in missing upstream ports) within the crate. It either writes the provided config or reads an archived one.
- **Runtime Agnostic**: The crate does not depend on `tokio`, `async`, or any specific networking/concurrency model. It uses standard synchronous I/O for file operations and memory mapping (`memmap2`), which is appropriate for a data format library.

## 2. Dependency Audit

| Dependency | Purpose | Boundary Assessment |
| :--- | :--- | :--- |
| `pavis-core` | Provides the `RuntimeConfig` structure. | **Safe**. Necessary for defining the high-level API (`load`/`write`). |
| `rkyv` | Zero-copy serialization engine. | **Safe**. Implementation detail of the binary format. |
| `sha2` | Checksum calculation (SHA-256). | **Safe**. Essential for format integrity. |
| `thiserror` | Error derivation. | **Safe**. |
| `memmap2` | Efficient file access. | **Safe**. Provides performance for large artifacts without introducing runtime complexity. |

## 3. Opaque Artifact Property

The `.pvs` artifact is designed to be treatable as an opaque unit by system components:

- **Relay Purity**: Components like `pavis-relay` can use `VerifiedPvs` or `inspect()` to validate and route artifacts based strictly on the 64-byte header (version, magic, checksum) without ever deserializing the inner configuration.
- **Validation Isolation**: The crate provides `verify_file()` and `read_from_path()`, which confirm the binary integrity (magic bytes, checksum, and `rkyv` archive structure) without requiring semantic understanding of the Pavis configuration.
- **Downstream Decoupling**: The runtime only needs to perform semantic validation *after* `pavis-pvs` has certified the artifact's structural integrity.

### Boundary Risk: None identified.
The crate maintains a clear separation between the "Container" (PVS) and the "Content" (Core).
