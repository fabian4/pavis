# PVS Binary Format Specification

> **Class:** SPECIFICATION  
> **Question:** What is the exact binary format of `.pvs` artifacts?  
> **Authority:** This document is normative. Implementation resides in code (`crates/pavis-pvs`).

---

## Overview

The `.pvs` file is the embodiment of the **Frozen Data Plane** - a rigid, byte-aligned binary artifact designed for streaming verification (checksum) followed by `rkyv` layout checks.

**Purpose:** To provide the runtime with a zero-copy, pre-validated memory image of configuration.

---

## Protocol Invariants

The PVS format enforces three critical invariants:

1. **Immutability**: Once written, a `.pvs` file is cryptographically sealed. Any modification invalidates the checksum.
2. **Completeness**: The artifact contains ALL information required for execution (except file-referenced TLS keys). No environment variables or dynamic lookups.
3. **Validity**: A valid PVS artifact implies the configuration has passed all semantic validation checks in the Codec.

---

## Binary Layout

### Header Structure (64 bytes, Little Endian)

```text
      0               4               8               12              16      
      +---------------+---------------+---------------+---------------+
0x00  |  Magic "PAVS" |  Version (u32)|   Algo (u32)  | Checksum (1/8)|      
      +---------------+---------------+---------------+---------------+
0x10  |                                                               |
...   |                   Checksum (32 bytes)                         |
0x20  |                                               | Checksum (8/8)|
      +---------------+---------------+---------------+---------------+
0x30  |                   Reserved / Padding (20 bytes)               |
      +---------------+---------------+---------------+---------------+
0x40  |                                                               |
...   |              RKYV ARCHIVE PAYLOAD (Variable)                  |
      |          (Relative Pointers, Aligned Data Segments)           |
      |                                                               |
      +---------------------------------------------------------------+
```

### Header Field Definitions

| Offset | Size | Type | Description |
|--------|------|------|-------------|
| `0x00` | 4 | `[u8; 4]` | **Magic Bytes**: `0x50 41 56 53` ("PAVS") |
| `0x04` | 4 | `u32` | **Schema Version**: Monotonically increasing. Mismatches MUST fail fast. |
| `0x08` | 4 | `u32` | **Algorithm ID**: `0x01` = SHA256 (default), `0x02` = XXHash3 |
| `0x0C` | 32 | `[u8; 32]` | **Checksum**: Hash of payload bytes (`0x40`...EOF) |
| `0x2C` | 20 | `[u8; 20]` | **Reserved/Padding**: Must be `0x00`. Ensures 64-byte header alignment. |
| `0x40` | N | `Bytes` | **Payload**: The archived `RuntimeConfig` using `rkyv` |

**Constants:**
- `HEADER_LEN = 0x40` (64 bytes)
- `PAYLOAD_OFFSET = 0x40`
- `PAVIS_MAGIC = [0x50, 0x41, 0x56, 0x53]` ("PAVS")
- `PAVIS_VERSION = 0` (current)

### Payload Format

The payload uses `rkyv`'s relative pointer architecture for zero-copy deserialization:

- **Invariant**: The `RuntimeConfig` root object is guaranteed at `PAYLOAD_OFFSET` (`0x40`)
- **Alignment**: `rkyv` handles internal alignment; the 64-byte header ensures proper offset
- **Validation**: Must pass `rkyv::check_bytes()` before access

---

## Verification Stages

PVS artifacts undergo three-stage verification:

1. **Header Validation**: Magic bytes, version, algorithm checks
2. **Checksum Verification**: Recompute payload hash and compare
3. **Archive Validation**: `rkyv::check_bytes()` for layout integrity

**Fail-Fast:** Any discrepancy results in immediate error:
- Wrong magic bytes → `InvalidMagic`
- Version mismatch → `VersionMismatch`
- Checksum mismatch → `ChecksumMismatch`
- Corrupt payload → `CorruptArchive`

---

## Serialization Process

Artifact creation follows this sequence:

1. Serialize `RuntimeConfig` using `rkyv::to_bytes()`
2. Compute SHA-256 checksum of serialized payload
3. Construct header with magic, version, algorithm, and checksum
4. Write header (64 bytes) + payload to file

---

## Version Compatibility

The runtime enforces strict version matching:

- **Newer version** → Runtime refuses to load (update runtime required)
- **Older version** → Runtime refuses to load (regenerate artifact required)
- **Matching version** → Proceed with loading

Version mismatches are **non-recoverable** and require either runtime upgrade or artifact regeneration.

---

## Security Properties

### Integrity

- **Guaranteed**: SHA-256 checksums detect any corruption or modification
- **Scope**: Covers entire payload (offset `0x40` to EOF)

### Confidentiality

- **NOT PROVIDED**: PVS files are unencrypted (intentional design choice)
- **Mitigation**: Apply external encryption if confidentiality required

### Authenticity

- **NOT PROVIDED**: Checksums detect corruption but not malicious tampering by authorized publishers
- **Future**: Reserved header bytes allow for signature fields

---

## Limitations

### Explicitly Not Supported

- **Encryption**: Protocol provides integrity only, not confidentiality
- **Compression**: No built-in compression (apply externally if needed)
- **Streaming Deserialization**: Full payload must be in memory for checksum verification
- **Partial Updates**: Artifacts are atomic; no incremental modification

### Future Extensions

The reserved 20 bytes (`0x2C` to `0x3F`) provide space for protocol extensions:

- Signature fields for authenticity verification
- Compression algorithm indicators
- Additional metadata (timestamps, author)

Current version (`0`) requires all reserved bytes to be zero.

---

## Related Documents

- **Architecture**: See `/ARCHITECTURE.md` for system invariants
- **Configuration Guide**: See `../configuration/guide.md` for artifact generation with `pavctl`
