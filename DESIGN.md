# Pavis Implementation Specification

> **Document Status:** Active Specification
> **Target Audience:** Core Protocol Engineers, Systems Programmers
> **Enforcement:** CI/CD Static Analysis, Code Review

## 1. The PVS Binary Protocol Specification

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

---

## 2. Runtime Memory Lifecycle (RCU Pattern)

Pavis achieves lock-free hot reloading using a Read-Copy-Update (RCU) pattern via `arc-swap`.

### 2.1 Core Structures

```rust
/// The main handle for the application state.
pub struct PavisEngine {
    config: ArcSwap<ConfigGuard>,
}

/// A RAII guard that manages the lifecycle of the mmap.
pub struct ConfigGuard {
    /// The memory-mapped file handle. 
    /// Dropping this struct calls munmap().
    _mmap: Mmap,
    
    /// Accessor for the archived data.
    /// The lifetime of the returned reference is tied to the mmap.
    /// Implementation uses a raw pointer or self-referential pattern internally.
    data_ptr: *const ArchivedRuntimeConfig,
}
```

### 2.2 The Atomic Swap Lifecycle

**[SAFETY]** All `unsafe` code (mmap creation, pointer casting, structure validation) is strictly confined to this "Swap" phase. The request path operates only on safe, validated references.

1.  **Stage:** The **Pavis Runtime** (via the background Long-Poll task) downloads the new `.pvs` file to a temporary location (e.g., `/tmp/pavis.next.pvs`).
2.  **Verify:**
    *   Open file.
    *   Validate Magic Bytes (`PAVS`).
    *   Compute Checksum matches Header.
    *   **[CRITICAL]** Perform `rkyv::check_archived_root::<RuntimeConfig>(bytes)` at `PAYLOAD_OFFSET` to validate structure integrity.
3.  **Map:** Call `mmap` on the valid file.
4.  **Swap:**
    ```rust
    let new_guard = ConfigGuard::new(mmap_handle); // unsafe block inside
    // Atomic pointer swap.
    engine.config.store(Arc::new(new_guard));
    ```
5.  **Reclaim:** The old `Arc<ConfigGuard>` is dropped. When the last request RefCount hits 0, `munmap` is invoked.

---

## 3. The Routing Algorithm (Hot Path)

Routing is hierarchical to minimize CPU cycles. We trade memory size for lookup speed.

### 3.1 Logical Structure
The Router structure is optimized for this specific evaluation order:

*   **Exact Match Table:** O(1) lookup for `(Host, Path)` combinations.
*   **Prefix Tree:** O(log N) radix tree for path matching.
*   **Regex Pattern List:** Ordered list of regex patterns.

### 3.2 Evaluation Logic
For every incoming request `req`:

1.  **Exact Match:**
    *   Construct lookup key `(req.host(), req.path())`.
    *   Check Exact Match Table.
2.  **Prefix Match:**
    *   Traverse Prefix Tree using `req.path()`.
3.  **Regex Match:**
    *   **[PERF]** Execute `RegexSet::matches(req.path())`.
    *   Priority is determined by index.

**[INVARIANT]** Regex compilation (`RegexSet::new()`) occurs **once** during the "Swap" phase (Section 2.2). The compiled regex state lives in runtime-only wrappers (e.g. `ConfigGuard` auxiliary fields), **NOT** inside the `.pvs` file. It must **NEVER** happen on the request path.

---

## 4. Relay Distribution Protocol (The State Machine)

The Relay ensures configuration propagation via HTTP Long-Polling.

### 4.1 Client/Server Handshake
*   **Request:** `GET /v1/config`
*   **Header:** `X-Pavis-Artifact-Version: <u64>`
    *   This is the **Artifact Version** (distribution version), distinct from the **PVS Schema Version** (binary compatibility).
*   **Response on Timeout:** `204 No Content`

### 4.2 Relay State Machine (Server Side)
The server uses `tokio::sync::Notify` to handle concurrent waiters without thread exhaustion.

```rust
async fn handle_poll(req: Request, state: State) -> Response {
    let client_ver = req.header("X-Pavis-Artifact-Version").parse::<u64>();
    let current_ver = state.artifact_version().await;

    if client_ver != current_ver {
        return send_file(current_ver);
    }

    // [PERF] Park the task using Tokio waker. 0 CPU usage.
    let notified = state.notifier().notified();
    match timeout(Duration::from_millis(wait_ms), notified).await {
        Ok(_) => send_file(state.artifact_version().await),
        Err(_) => Response::builder().status(204).body(Empty) // 204 No Content
    }
}
```

---

## 5. Networking & Connection Pooling

### 5.1 Connection Pooling
*   **Key:** `(UpstreamIP, UpstreamPort, SNI)`.
*   **Reuse Strategy:** LIFO (Last-In-First-Out).
*   **Policies:** Idle timeout and connection limits are defined by `RuntimeConfig`. The Runtime MUST NOT invent default policies.

### 5.2 Implementation Hook
We implement `pingora::ProxyHttp`. 

*   `request_filter()`: Acquire `Arc<ConfigGuard>` -> Route -> Load Balance.
*   **[PERF]** This phase must be non-blocking. No IO allowed.

---

## 6. Implementation Guidelines

### 6.1 Safety & Allocations
*   **Allocations:** Zero-allocation on the hot path.
    *   Use `Cow<'_, str>` or references into the `mmap` region.
*   **Unsafe:**
    *   Allowed **ONLY** for `mmap` and casting to `Archived` types.
    *   Must have a `// SAFETY: ...` comment.
*   **Panic Policy:**
    *   `panic="abort"` in release profile.
    *   NEVER panic on the request path. Return `500`.

### 6.2 Telemetry
*   **Metrics:** Use atomic counters (`std::sync::atomic`).
*   **Logging:** Structural logging only. No formatted strings in hot loops.
