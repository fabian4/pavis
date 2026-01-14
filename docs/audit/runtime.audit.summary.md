Audit Phase: Runtime Audit
Target Crate: crates/pavis
Generation Timestamp: 2026-01-14T12:10:00Z
AI Model: Gemini

# 1. Executive Verdict

**Verdict:** Sound

The `pavis` runtime is a high-performance, frozen data plane implementation that strictly adheres to the architectural constraints. It consumes opaque, pre-validated `.pvs` artifacts, enforcing a clean separation between configuration logic (Core) and execution (Runtime). Concurrency is handled safely via `ArcSwap` for hot reloads and lock-free atomics for load balancing. While there is a minor consistency risk during hot reloads (request phases potentially seeing different config snapshots) and some allocation overhead in the request ID path, the codebase is structurally sound, panic-free in hot paths, and ready for production lock-in.

# 2. Top System Risks

1.  **Reload Consistency (Phase 2):**
    The `Proxy` service re-loads the configuration state (`self.state.load()`) multiple times during a single request lifecycle (once in `request_filter` for routing, and again in `upstream_peer` for backend selection).
    *   *Risk:* A hot reload occurring mid-request could cause the routing phase to select an upstream from Config A, while the peer selection phase tries to resolve it in Config B. If the upstream was removed or changed in Config B, the request will fail or misroute.
2.  **Request ID Allocation (Phase 5):**
    `generate_request_id` allocates a new `String` for every request using `format!`.
    ```rust
    // crates/pavis/src/proxy/service.rs
    format!("req-{}-{}", now, random_val)
    ```
    *   *Risk:* Unnecessary heap churn in the ultra-hot path.
3.  **Regex Construction Safety (Phase 4):**
    While `Router::new` compiles regexes safely, a malformed regex in a `RuntimeConfig` (if valid PVS but invalid regex syntax, though `pavis-core` should catch this) would cause `Router::new` to fail.
    *   *Mitigation:* `pavis-core` validation ensures regex validity, so this is a layered defense relying on the trusted producer.

# 3. Readiness Assessment

| Criteria | Status | Notes |
| :--- | :--- | :--- |
| **Boundary Purity?** | **Yes** | Runtime consumes `.pvs` only. No semantic validation or parsing logic exists. |
| **Runtime Invariants?** | **Mostly** | State is immutable, but request-scoped consistency is not strictly enforced (see Risk #1). |
| **Diagnosable Errors?** | **Yes** | Tracing spans cover request lifecycle. `anyhow` used for startup errors. |
| **Concurrency Safety?** | **Yes** | `ArcSwap` handles config swaps. `AtomicUsize` handles LB state. No blocking locks in hot paths. |
| **Performance Risks?** | **Low** | Main overhead is allocation (Request ID, Rewrites). Routing is efficient (Linear+Map). |

# 4. Recommended Next Steps

1.  **Pin Config per Request:** Modify `RouterContext` to hold an `Arc<RuntimeState>` captured at the start of the request (`request_filter`). Pass this snapshot to `upstream_peer` to ensure a request is processed entirely within a single configuration version.
2.  **Optimize Request ID:** Replace `String` allocation with a thread-local formatter or a fixed-size buffer (e.g., `compact_str` or `ulid`) to reduce heap pressure.
3.  **Validate Regex compilation:** Ensure `pavis-core`'s regex validation is strictly aligned with `runtime`'s regex engine (`regex` crate) to prevent "valid at core, invalid at runtime" scenarios.

# 5. Detailed Analysis

## Phase 0: Inventory & Runtime Surface
-   **Architecture:** `Pingora` based proxy. `Router` for matching. `Manager` for upstream selection. `Telemetry` for observability.
-   **Entry:** `main.rs` loads LKG config, bootstraps server, and starts `ConfigAgent`.
-   **Components:** Clean separation of concerns. `proxy/service.rs` orchestrates, but logic delegates to `router` and `upstream`.

## Phase 1: Boundary & Responsibility Audit
-   **Input:** `load_file` accepts only `.pvs` extension and uses `pavis_pvs::load`.
    ```rust
    // crates/pavis/src/load.rs
    pub fn load_file(path: &str) -> LoadResult<ValidatedRuntimeConfig> { ... }
    ```
-   **Parsing:** **PASSED.** No YAML/JSON parsing found.
-   **Defaults:** **PASSED.** Runtime fails if config is missing required data (e.g. "Upstream not found"), effectively enforcing that the config provided is complete.

## Phase 2: Runtime Invariants & Correctness
-   **Immutability:** `RuntimeState` is read-only.
-   **Hot Reload:** `ArcSwap` provides atomic pointer swap.
-   **Determinism:** `Router` implementation strictly respects order of routes (Linear zones preserve vector order, Map zones preserve first-insert).

## Phase 3: Error Model & Diagnostics
-   **Startup:** `anyhow` provides good context for bind failures or config load errors.
-   **Runtime:** `tracing` spans provide `request_id`, `route_pattern`, `upstream`.
-   **Panic Policy:** Hot paths in `proxy.rs` use `Result` (via `pingora::Error`). No panic risks observed.

## Phase 4: Safety, Concurrency & Hot Reload
-   **Concurrency:** `AtomicUsize` (Relaxed) used for Round-Robin counter. Safe and fast.
    ```rust
    // crates/pavis/src/upstream/load_balance.rs
    let val = counter.fetch_add(1, Ordering::Relaxed);
    ```
-   **Memory Safety:** `Arc` usage ensures config data remains alive as long as requests need it. No `unsafe` blocks found in `src/pavis` (delegated to libraries).

## Phase 5: Performance & Latency Signals
-   **Routing:** `Router::match_request` uses a hybrid approach (HashMap for exact paths, Vector for regex/prefix). This optimizes for the common case (Exact) while supporting complex matching.
-   **Allocation:**
    -   `extract_client_identity` allocates `String`.
    -   `generate_request_id` allocates `String`.
    -   `calculate_path_rewrite` allocates `String` / `Cow`.
-   **Locking:** No `Mutex` or `RwLock` in the request path.