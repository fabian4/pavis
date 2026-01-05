## 📌 Overall Summary (Latest)

🚫 Blocker: 0 · 🔥 High: 1 · ⚠️ Medium: 2 · 🧹 Low: 2 · ✅ Resolved: 0

---

## Open Findings (Prioritized)

| ID  | Severity | Area | Short Title |
|----:|:--------:|------|-------------|
| F-2 | High | Request Path | Unnecessary path allocation in proxy hot path |
| F-3 | Medium | Telemetry | Access log formatting on request path |
| F-4 | Medium | Routing | O(N) linear scan for VirtualHost matching |
| F-1 | Low | Startup Allocation | PVS loading reads entire file into heap |
| F-5 | Low | Telemetry | Synchronous I/O in AccessLogWorker startup |

---

## Review Entry — 2026-01-05T10:30:00Z

### Scope
- Performance and allocation review of `pavis` runtime (proxy, router, upstream, telemetry).

---

### Method
- Static analysis of hot paths for allocations (`clone`, `to_string`, `Vec::new`).
- Review of complexity in matching and load balancing algorithms.
- Audit of async/blocking boundaries.

### Model
- gemini-2.0-flash-thinking-exp

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-2 | High | Request Path | Unnecessary path allocation in proxy hot path | Open |
| F-3 | Medium | Telemetry | Access log formatting on request path | Open |
| F-4 | Medium | Routing | O(N) linear scan for VirtualHost matching | Open |
| F-5 | Low | Telemetry | Synchronous I/O in AccessLogWorker startup | Open |

---

### Detailed Findings

#### F-2: Unnecessary path allocation in proxy hot path
- **Expectation:** Request path lookup should be zero-allocation using slices.
- **Observed:** Every request allocates a new `String` from the URI path.
- **Evidence:** `crates/pavis/src/proxy/service.rs:214`: `let uri_path = req_header.uri.path().to_string();`
- **Impact:** High — Unnecessary heap pressure and copy overhead on every single request. The matcher already accepts `&str`.
- **Recommendation:** Pass `req_header.uri.path()` directly to `match_request`.

#### F-3: Access log formatting on request path
- **Expectation:** Telemetry formatting should happen off the hot path in a background task.
- **Observed:** `format_log_line` (which allocates a `String`) is called before the log is sent to the worker channel.
- **Evidence:** `crates/pavis/src/telemetry/access_log.rs:142`
- **Impact:** Medium — Adds latency to the request processing before the session can return.
- **Recommendation:** Send a structured log entry to the channel and format it in the `AccessLogWorker` loop.

#### F-4: O(N) linear scan for VirtualHost matching
- **Expectation:** Host matching should use efficient lookups (e.g., HashMap) for fixed domains.
- **Observed:** `match_request` iterates through all virtual hosts twice (once for exact, once for wildcards).
- **Evidence:** `crates/pavis/src/router/matcher.rs:34, 41, 55`
- **Impact:** Medium — Routing performance degrades linearly with the number of configured domains.
- **Recommendation:** Index non-wildcard vhosts in a `HashMap` for O(1) lookup.

#### F-5: Synchronous I/O in AccessLogWorker startup
- **Expectation:** Async services should use async I/O or spawn blocking tasks for initialization.
- **Observed:** `std::fs::OpenOptions` is used directly in `start_service`.
- **Evidence:** `crates/pavis/src/telemetry/access_log.rs:31`
- **Impact:** Low — Minor reactor stall during worker initialization.
- **Recommendation:** Use `tokio::fs::OpenOptions`.

---

## Review Entry — 2026-01-01T03:11:42Z

### Scope
- Repository-wide performance and allocation review.

---

### Method
- Analysis of startup paths, hot paths, and allocation patterns.

### Model
- claude-sonnet-4-20250514

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Low | Startup Allocation | PVS loading reads entire file into heap | Open |

---

### Detailed Findings

#### F-1: PVS loading reads entire file into heap (unchanged)
- **Expectation:** Zero-copy or streaming loading for large configs.
- **Observed:** `pavis_pvs::load` uses `fs::read`, allocating full file to heap.
- **Evidence:** `crates/pavis-pvs/src/verify.rs`
- **Impact:** Low — config files typically small; roadmap tracks mmap optimization.
- **Status:** Open (Low) — tracked in roadmap Phase 3 "Enable Zero-Copy".

#### Performance Strengths Observed

**Hot Path Efficiency:**
- ✅ Router uses pre-compiled regexes (not compiled per-request)
- ✅ `ArcSwap` for lock-free config reads
- ✅ Load balancer uses `AtomicU64` counters (no locks)
- ✅ Access log uses non-blocking channel (`try_send`)

**Startup Path:**
- ✅ Config loaded once at startup
- ✅ Runtime state built from validated config
- ✅ Regex compilation happens during state initialization

**Request Path:**
- ✅ No allocations for routing decisions
- ✅ Header operations reuse existing types
- ✅ Telemetry uses pre-allocated buffers

No additional performance issues identified.

---

## Review Entry — 2025-12-30T11:35:29Z

### Scope
- Repository-wide performance and allocation scan.

---

### Method
- Manual scan of startup/config-loading paths and relay hot paths for allocation-heavy patterns.


### Model
- GPT-5

---

### Summary (Index)

No new findings. Existing startup allocation issue remains the primary performance concern.

---

## Review Entry — 2025-12-30T05:02:44Z

### Scope
- Startup path and hot-path request routing review.

---

### Method
- Manual analysis of allocation patterns in `pavis-pvs` and `pavis` router.


### Model
- gemini-2.0-flash-exp

---

### Summary (Index)

| ID  | Severity | Area | Short Title | Status |
|----:|:--------:|------|-------------|:------:|
| F-1 | Low | Startup Allocation | PVS loading reads entire file into heap | Open |

---

### Detailed Findings

#### F-1: PVS loading reads entire file into heap
- **Expectation:** Configuration loading should ideally be zero-copy or streaming to support large files.
- **Observed:** `pavis_pvs::load` and `verify` use `fs::read`, which allocates a `Vec<u8>` for the entire file content.
- **Evidence:** `crates/pavis-pvs/src/verify.rs` calls `fs::read(path)`.
- **Assessment (Reason):** Limits scalability for very large configuration files (heap exhaustion risk).
- **Recommendation (Suggestion):** Implement `mmap` based loading (aligned with Roadmap Phase 2).
- **Doc Drift?:** No — aligned with known roadmap optimization phase.
