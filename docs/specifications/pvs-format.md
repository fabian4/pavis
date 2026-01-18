# Frozen Artifact Specification (PVS Protocol)

The `.pvs` file is the embodiment of the **Frozen Data Plane**. It is a rigid, byte-aligned binary artifact designed for streaming verification (checksum) followed by `rkyv` layout checks.

**Purpose**: To provide the runtime with a zero-copy, pre-validated memory image of the configuration.

## 1. Artifact Invariants

1.  **Immutability**: Once written, a `.pvs` file is cryptographically sealed.
2.  **Completeness**: The artifact contains ALL information required for execution. No external file references (except TLS keys), no environment variables, no dynamic lookups.
3.  **Validity**: A valid PVS artifact implies that the configuration has passed all semantic validation checks in the Codec.

## 2. Binary Layout (Little Endian)

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

**Constants:**
- `HEADER_LEN = 0x40`
- `PAYLOAD_OFFSET = 0x40`

| Offset | Size | Type | Description |
| :--- | :--- | :--- | :--- |
| **0x00** | 4 | `[u8; 4]` | **Magic Bytes:** `0x50 41 56 53` ("PAVS") |
| **0x04** | 4 | `u32` | **Schema Version:** Monotonically increasing integer. Mismatches MUST fail fast. |
| **0x08** | 4 | `u32` | **Algo ID:** `0x01` = SHA256 (Default), `0x02` = XXHash3. |
| **0x0C** | 32 | `[u8; 32]` | **Checksum:** The hash of the payload bytes (0x40...EOF). |
| **0x2C** | 20 | `[u8; 20]` | **Reserved/Padding:** Must be `0x00`. Ensures Payload starts at 64-byte alignment. |
| **0x40** | N | `Bytes` | **Payload:** The archived `RuntimeConfig` root object. |

## 3. The Rkyv Payload
The payload utilizes `rkyv`'s relative pointer architecture.
*   **[INVARIANT]** The `RuntimeConfig` root object is guaranteed to be located at `PAYLOAD_OFFSET` (`0x40`).
