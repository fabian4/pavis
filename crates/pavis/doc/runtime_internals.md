# Runtime Internals Specification

> **Status:** Implementation Specification
> **Role:** Describes the Pavis Runtime memory lifecycle, routing algorithms, and resource management.

## 1. Runtime Memory Lifecycle (RCU Pattern)

Pavis achieves lock-free hot reloading using a Read-Copy-Update (RCU) pattern via `arc-swap`.

### 1.1 Core Structures

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

### 1.2 The Atomic Swap Lifecycle

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

## 2. The Routing Algorithm (Hot Path)

Routing is hierarchical to minimize CPU cycles. We trade memory size for lookup speed.

### 2.1 Logical Structure
The Router structure is optimized for this specific evaluation order:

*   **Exact Match Table:** O(1) lookup for `(Host, Path)` combinations.
*   **Prefix Tree:** O(log N) radix tree for path matching.
*   **Regex Pattern List:** Ordered list of regex patterns.

### 2.2 Evaluation Logic
For every incoming request `req`:

1.  **Exact Match:**
    *   Construct lookup key `(req.host(), req.path())`.
    *   Check Exact Match Table.
2.  **Prefix Match:**
    *   Traverse Prefix Tree using `req.path()`.
3.  **Regex Match:**
    *   **[PERF]** Execute `RegexSet::matches(req.path())`.
    *   Priority is determined by index.

**[INVARIANT]** Regex compilation (`RegexSet::new()`) occurs **once** during the "Swap" phase (Section 1.2). The compiled regex state lives in runtime-only wrappers (e.g. `ConfigGuard` auxiliary fields), **NOT** inside the `.pvs` file. It must **NEVER** happen on the request path.

## 3. Networking & Connection Pooling

### 3.1 Connection Pooling
*   **Key:** `(UpstreamIP, UpstreamPort, SNI)`.
*   **Reuse Strategy:** LIFO (Last-In-First-Out).
*   **Policies:** Idle timeout and connection limits are defined by `RuntimeConfig`. The Runtime MUST NOT invent default policies.

### 3.2 Implementation Hook
We implement `pingora::ProxyHttp`. 

*   `request_filter()`: Acquire `Arc<ConfigGuard>` -> Route -> Load Balance.
*   **[PERF]** This phase must be non-blocking. No IO allowed.

## 4. Current Constraints & Limitations

The current runtime implementation enforces specific constraints to maintain simplicity and determinism.

1.  **Single Listener**:
    *   The `server` block supports a single listening address.
    *   If multiple configurations are provided or merged, the runtime processes only the *first valid listener*.
2.  **TLS Configuration**:
    *   **File Paths Only**: TLS certificates and keys must be referenced via file system paths (`cert_path`, `key_path`).
    *   **No Inline Certificates**: Inline PEM strings are not supported.
3.  **Upstream Resolution**:
    *   **IP-Only Endpoints**: Upstream clusters currently support only explicit IP-based `Endpoint` definitions.
    *   **No DNS Support**: Logical DNS (`LOGICAL_DNS`) and Strict DNS (`STRICT_DNS`) cluster types are **not currently supported**. Resolution must happen at the Ingest/Control Plane layer.
4.  **Header Operations**:
    *   **Insert/Overwrite Behavior**: The `add` operation in `HeaderOperations` functions as an **insert/overwrite**.
    *   **Append Ignored**: The "append" flag is currently ignored.
5.  **Unsupported Route Actions**:
    *   `DirectResponse`, `Redirect`, and `HostRewrite`/`PathRewrite` are currently **unsupported**.