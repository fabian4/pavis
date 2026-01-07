Audit phase name: Phase 2: Format Invariants & Correctness
Target crate: crates/pavis-pvs
Generation timestamp: 2026-01-07T10:37:00Z
AI model identifier: unknown

# Phase 2: Format Invariants & Correctness

## 1. Invariant Inventory

| Invariant Name | Involved Types | Description |
| :--- | :--- | :--- |
| **Magic Identification** | `PvsHeader.magic` | Every PVS file must begin with the 4-byte sequence `b"PAVS"`. |
| **Version Compatibility** | `PvsHeader.version` | The version in the header must exactly match `PAVIS_VERSION` (currently 0). |
| **Hash Algorithm** | `PvsHeader.algorithm` | The algorithm ID must be `PAVIS_HASH_ALGORITHM_SHA256` (1). |
| **Header Size** | `HEADER_SIZE` | The header is always exactly 64 bytes long. |
| **Payload Integrity** | `PvsHeader.checksum` | The payload following the header must have a SHA-256 hash matching the checksum in the header. |
| **Archive Safety** | `rkyv::check_archived_root` | The payload must be a valid, well-formed `rkyv` archive for the `RuntimeConfig` type. |

## 2. Enforcement Matrix

| Invariant | Enforced Location | Mechanism | Bypass Risk |
| :--- | :--- | :--- | :--- |
| **Magic Bytes** | `src/verify.rs`: `verify_bytes` | Equality check against `PAVIS_MAGIC`. | No |
| **Version** | `src/verify.rs`: `verify_bytes` | Equality check against `PAVIS_VERSION`. | No |
| **Algorithm** | `src/verify.rs`: `verify_bytes` | Equality check against `PAVIS_HASH_ALGORITHM_SHA256`. | No |
| **Checksum** | `src/verify.rs`: `verify_bytes` | Re-computation of SHA-256 over `bytes[HEADER_SIZE..]`. | No |
| **Archive Safety** | `src/verify.rs`: `verify_file`, `load` | `rkyv::check_archived_root` validates offsets and bounds. | No |

## 3. Gaps

- **Reserved Bytes are Unchecked**: The 20 reserved bytes in `PvsHeader` are set to zero by `write.rs` but are not verified by `verify_bytes` in `verify.rs`. This allows arbitrary data to be stored in the header without being detected, which may cause compatibility issues if these bytes are used for future features.
- **No Payload Length Field**: The PVS format relies on the filesystem or transport layer to provide the total length of the file/buffer. There is no `payload_length` field in the header to detect truncation if the transport layer lies about the size (though the checksum will detect this).
- **No Header Checksum**: While the checksum field protects the payload, and field mismatches (magic/version) are checked, there is no explicit checksum for the 64-byte header itself. Corruptions in the `_reserved` field are completely undetected.
- **Payload Size Upper Bound**: There is no enforced maximum size for the PVS payload. While limited by memory/address space, a maliciously large file could lead to resource exhaustion (DoS) during the verification/hashing phase.
