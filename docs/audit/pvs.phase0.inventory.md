Audit phase name: Phase 0: Inventory & Artifact Surface
Target crate: crates/pavis-pvs
Generation timestamp: 2026-01-07T10:33:00Z
AI model identifier: unknown

# Phase 0: Inventory & Artifact Surface

## 1. File Inventory

| File Path | Responsibility | Description |
| :--- | :--- | :--- |
| `src/lib.rs` | helpers, tests | Top-level module re-exporting the public API and defining common constants for HTTP headers. |
| `src/header.rs` | format/layout, header/versioning | Defines the `PvsHeader` structure, magic bytes, version constants, and basic checksum helpers. |
| `src/error.rs` | other | Defines the `PvsError` enum for all PVS-related failures. |
| `src/read.rs` | header/versioning, integrity checks | Logic for reading and parsing the PVS header from files or byte buffers. |
| `src/write.rs` | serialization/deserialization | Logic for encoding a `RuntimeConfig` into the binary PVS format (header + rkyv payload). |
| `src/verify.rs` | integrity checks, serialization/deserialization | The core verification engine that validates magic bytes, version, checksums, and rkyv archive integrity. |

## 2. Module Structure

The crate is organized into functional modules for each stage of the artifact lifecycle:
- **`header`**: The static definition of the binary format's leading bytes.
- **`error`**: Error taxonomy for validation and I/O failures.
- **`read` / `write`**: Transformation logic between memory and the binary format.
- **`verify`**: Validation logic for integrity and safety, including support for memory-mapped files.

## 3. Public API Surface

### Structs
- `PvsHeader` (`src/header.rs`): The fixed-size C-repr header of a PVS file.
- `PvsHeaderView` (`src/verify.rs`): A read-only view of a parsed header, typically returned by inspection.
- `VerifiedPvs` (`src/verify.rs`): A handle to a fully validated PVS artifact, containing the header and either owned or memory-mapped bytes.

### Enums
- `PvsError` (`src/error.rs`): Errors covering I/O, format violations, version mismatches, and checksum failures.

### Type Aliases
- `PvsResult<T>` (`src/error.rs`): Standard result alias for the crate.

### Functions
- `compute_checksum` (`src/header.rs`): Computes the SHA-256 checksum for a payload.
- `checksum_hex` (`src/header.rs`): Formats a checksum as a hex string.
- `algorithm_label` (`src/header.rs`): Returns a human-readable string for the hash algorithm ID.
- `read_header` (`src/read.rs`): Reads the header from a file path.
- `encode` (`src/write.rs`): Serializes a `RuntimeConfig` into a PVS-formatted byte vector.
- `write` (`src/write.rs`): Serializes and writes a `RuntimeConfig` to a file.
- `inspect` (`src/verify.rs`): Quickly parses and returns header information from a byte buffer.
- `verify` (`src/verify.rs`): Fully validates a byte buffer as a PVS artifact.
- `read_from_path` (`src/verify.rs`): Memory-maps and validates a PVS file from disk.
- `verify_file` (`src/verify.rs`): Validates a PVS file on disk without returning the payload.
- `load` (`src/verify.rs`): Fully validates and deserializes a PVS file into a `RuntimeConfig`.

### Constants
- `PAVIS_MAGIC` (`src/header.rs`): The 4-byte magic identifier `b"PAVS"`.
- `PAVIS_VERSION` (`src/header.rs`): The current supported protocol version.
- `HEADER_SIZE` (`src/header.rs`): Fixed size of the header (64 bytes).
- `PAVIS_HASH_ALGORITHM_SHA256` (`src/header.rs`): Identifier for the SHA-256 algorithm.
- `PAVIS_VERSION_HEADER` (`src/lib.rs`): HTTP header name for the PVS version.
- `PAVIS_CHECKSUM_HEADER` (`src/lib.rs`): HTTP header name for the PVS checksum.
- `PAVIS_CHECKSUM_ALG_HEADER` (`src/lib.rs`): HTTP header name for the checksum algorithm.
- `PAVIS_GENERATED_AT_HEADER` (`src/lib.rs`): HTTP header name for the artifact generation timestamp.
