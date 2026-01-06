# Pavis PVS Protocol

## 1. Crate Overview
`pavis-pvs` implements the binary protocol boundary for the Pavis ecosystem. It defines the structure of `.pvs` configuration files, handling their serialization, deserialization, and integrity verification. It acts as the gatekeeper for configuration artifacts, ensuring they are tamper-evident and compatible with the runtime.

Its primary responsibilities are:
- Defining the 64-byte PVS header structure.
- Serializing `RuntimeConfig` objects into the binary format with computed checksums.
- Reading and validating `.pvs` files, including header checks and payload verification.
- Providing memory-mapped I/O support for efficient loading of large configurations.

It explicitly does not handle:
- Logic for parsing human-readable formats (YAML/JSON).
- Semantic validation of the configuration content (delegated to `pavis-core`).

## 2. Features
- **Tamper-Evident Format**: Every `.pvs` file includes a SHA-256 checksum of the payload in its header, verified upon loading.
- **Zero-Copy Serialization**: Leverages `rkyv` for high-performance, alignment-aware serialization of the configuration payload.
- **Memory-Mapped Loading**: Supports `mmap` via the `memmap2` crate to load configuration files instantly without copying the entire payload into heap memory.
- **Strict Versioning**: Enforces protocol version matching to prevent runtime incompatibility.

## 3. Module Breakdown

### `header`
Defines the `PvsHeader` struct and constants (`PAVIS_MAGIC`, `PAVIS_VERSION`). It includes logic for computing and formatting checksums.

### `read`
Handles the low-level reading of the header from files or byte slices. It performs initial checks on file size and header bounds.

### `write`
Responsible for the creation of `.pvs` artifacts. It serializes the `RuntimeConfig` using `rkyv`, computes the checksum, and writes the complete binary structure (header + payload) to disk.

### `verify`
Implements the full verification pipeline.
- `verify`: Checks magic bytes, version, and checksum for in-memory bytes.
- `verify_file`: Performs the same checks on a file path using memory mapping.
- `VerifiedPvs`: A wrapper that holds validated data, ensuring access only occurs after integrity checks pass.

### `error`
Defines `PvsError` to categorize failures such as `ChecksumMismatch`, `VersionMismatch`, or `CorruptArchive`.

## 4. Public API Surface

### `write`
`pub fn write(path: impl AsRef<Path>, config: &RuntimeConfig) -> PvsResult<()>`
Encodes and writes a configuration to the specified path.

### `load`
`pub fn load(path: impl AsRef<Path>) -> PvsResult<RuntimeConfig>`
Loads, verifies, and deserializes a configuration from a file, returning the usable `RuntimeConfig`.

### `verify_file`
`pub fn verify_file(path: impl AsRef<Path>) -> PvsResult<()>`
Checks the integrity of a file without deserializing the full payload, useful for quick health checks.

### `VerifiedPvs`
A trusted container for PVS data.
- `header()`: Access metadata.
- `bytes()`: Access the verified raw payload.

## 5. Configuration and Runtime Behavior
This crate does not use external configuration; its behavior is defined by constants in the `header` module.

### Protocol Constants
- **Magic**: `PAVS` (0x50415653)
- **Version**: `0` (current)
- **Algorithm**: `1` (SHA-256)

### Runtime Behavior
- **Fail-Fast**: Any discrepancy in magic bytes, version, or checksum results in an immediate error.
- **Atomic Writes**: While the crate writes to the specified path, callers are responsible for atomic rename operations if needed (though `pavctl` handles this).

## 6. Error Handling and Invariants

### Error Types
- `ChecksumMismatch`: The payload hash does not match the header.
- `VersionMismatch`: The file version is newer or older than the runtime supports.
- `InvalidMagic`: The file is not a PVS artifact.
- `CorruptArchive`: The `rkyv` payload is invalid (e.g., alignment issues or truncated data).

### Invariants
- **Header Size**: Fixed at 64 bytes.
- **Alignment**: The payload immediately follows the header. `rkyv` handles internal alignment, but the file offset is fixed.

## 7. Non-Goals and Explicit Limitations
- **Encryption**: The protocol provides integrity (checksums) but not confidentiality. PVS files are unencrypted.
- **Compression**: The format does not currently support compression (zstd/gzip) at the protocol level.
- **Streaming**: Verification requires access to the full payload to compute the checksum; it is not designed for streaming usage.
