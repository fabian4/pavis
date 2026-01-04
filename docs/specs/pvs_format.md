# PVS Binary Protocol Specification

> **Status:** Implementation Specification
> **Role:** Defines the `.pvs` file layout and integrity checks.

## 1. The PVS Binary Protocol

The `.pvs` file is a rigid, byte-aligned binary artifact designed for direct memory mapping (`mmap`). It allows the data plane to access configuration data with minimal overhead.

### 1.1 Binary Layout (Little Endian)

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
| **0x04** | 4 | `u32` | **Schema Version:** Monotonically increasing integer. Mismatches MUST fail fast during load/reload. MUST NOT panic on the request path. |
| **0x08** | 4 | `u32` | **Algo ID:** `0x01` = SHA256 (Default), `0x02` = XXHash3. |
| **0x0C** | 32 | `[u8; 32]` | **Checksum:** The hash of the payload bytes (0x40...EOF). |
| **0x2C** | 20 | `[u8; 20]` | **Reserved/Padding:** Must be `0x00`. Ensures Payload starts at 64-byte alignment (Cache Line). |
| **0x40** | N | `Bytes` | **Payload:** The archived `RuntimeConfig` root object. |

### 1.2 The Rkyv Payload
The payload utilizes `rkyv`'s relative pointer architecture. Unlike absolute pointers (which require relocation logic on load), relative pointers encode offsets.

*   **[INVARIANT]** The `RuntimeConfig` root object is guaranteed to be located at `PAYLOAD_OFFSET` (`0x40`).
