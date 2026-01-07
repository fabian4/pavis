# Specifications

> **Role:** Normative Protocols and File Formats.

## 1. Frozen Artifact Specification (PVS Protocol)

The `.pvs` file is the embodiment of the **Frozen Data Plane**. It is a rigid, byte-aligned binary artifact designed for direct memory mapping (`mmap`).

**Purpose**: To provide the runtime with a zero-copy, pre-validated memory image of the configuration.

### 1.1 Artifact Invariants

1.  **Immutability**: Once written, a `.pvs` file is cryptographically sealed.
2.  **Completeness**: The artifact contains ALL information required for execution. No external file references (except TLS keys), no environment variables, no dynamic lookups.
3.  **Validity**: A valid PVS artifact implies that the configuration has passed all semantic validation checks in the Codec.

### 1.2 Binary Layout (Little Endian)

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

### 1.3 The Rkyv Payload
The payload utilizes `rkyv`'s relative pointer architecture.
*   **[INVARIANT]** The `RuntimeConfig` root object is guaranteed to be located at `PAYLOAD_OFFSET` (`0x40`).

---

## 2. Relay Distribution Protocol

The Relay distributes **Frozen Artifacts** via HTTP Long-Polling. It does not inspect the artifact content, serving only as a distribution mechanism.

### 2.1 State Machine (Server)

The server uses `tokio::sync::Notify` to handle concurrent waiters without thread exhaustion.

1.  Parse `X-Pavis-Artifact-Version` from request.
2.  Compare with Relay's `current_version`.
3.  **If** `client_ver != current_ver`: Immediate response (200 OK + File).
4.  **If** `client_ver == current_ver`:
    *   Register interest in `Notify` handle.
    *   Await `Notify` OR `Timeout`.
    *   **On Notify:** Response (200 OK + File).
    *   **On Timeout:** Response (204 No Content).

---

## 3. Configuration Alignment Plan (Frozen Purity)

This section records the alignment plan for enforcing the Frozen Data Plane model in the configuration pipeline.

### Short-term (Alignment & Safety)
1.  **Explicit Pipeline Boundary**: Enforce check → compile → materialize in `pavis-codec-api`. (Status: Completed)
2.  **Remove Semantic Defaults from Parsing**: Ensure `#[serde(default)]` does not inject business logic. (Status: Completed)
3.  **Isolate Structural Completion**: Separate shape normalization from semantic defaulting inside codec `compile`.

### Medium-term (Structural Clarity)
4.  **Constrain codec-api**: Ensure it enforces the boundary and core validation, with no semantic defaults.
5.  **Enforce RuntimeConfig Finality**: Runtime must reject configurations that haven't passed core validation.

### Long-term (Governor-readiness)
6.  **Harden Relay**: Ensure Relay treats `.pvs` blobs as opaque artifacts without inspection.
