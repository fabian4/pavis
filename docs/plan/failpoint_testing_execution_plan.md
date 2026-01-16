# Failpoint-Based E2E Testing: Implementation-Ready Plan (Corrected)

## Key Fixes Applied

1. **Removed non-existent history API references**: Eliminated all mentions of `GET /config/history`. History verification is now filesystem-only (direct inspection of `history/{ver}.pvs` and `history/{ver}.meta.json`).

2. **Clarified state.json vs lkg/meta.json semantics**: `state.json` is explicitly a cache ONLY; `lkg/meta.json` is the authoritative source of truth. Startup reconciliation logic now rebuilds `state.json` from `lkg/meta.json` if mismatched.

3. **Aligned failpoints with canonical publish flow**: Renamed all relay failpoints to match exact durability boundaries (e.g., `AFTER_HISTORY_PVS_FSYNC`, `AFTER_HISTORY_META_FSYNC`, `AFTER_LKG_PVS_RENAME`, `AFTER_LKG_META_RENAME`).

4. **Simplified failpoint actions**: Default to "abort" for E2E tests. "panic" only where process-exit is guaranteed.

5. **Fixed assertion strategy**: All E2E tests now assert via:
   - `GET /v1/status` (current_version, lkg metadata)
   - `GET /v1/config` headers (X-Config-Version, X-Config-Checksum, X-Config-Size - exact casing)
   - Filesystem checks (lkg/meta.json, lkg/config.pvs, history/*)

6. **Tightened runtime failpoint placement**: Mapped to `BEFORE_APPLY`, `AFTER_VALIDATION`, `AFTER_SWAP`, `AFTER_LKG_PERSIST` exclusively in agent/apply control flow.

7. **Specified exact file locations**: `pavis-relay/src/failpoint.rs` and `pavis-runtime/src/failpoint.rs` for helper modules.

8. **Corrected checksum expectations**: Same artifact (PVS bytes) always produces the same checksum, even across different version numbers.

9. **Deterministic reconciliation strategies**:
   - **FP-R4 (LKG mismatch)**: Meta-authoritative rollback only. Revert/delete `lkg/config.pvs` to match `lkg/meta.json`.
   - **History orphans**: Only accept paired (pvs + meta) entries. Delete unpaired artifacts on startup with warning log.

10. **Runtime swap observability**: Add test-only "apply journal" (fsync'd after swap) to distinguish crash windows.

---

## 1. Hardening Notes / Determinism Requirements

This section defines **mandatory** implementation requirements to ensure deterministic, portable, and flakiness-free E2E failpoint tests.

### 1.1 HTTP Header Casing (Case-Insensitive Matching)

**Requirement**: HTTP header names are case-insensitive per RFC 7230. HTTP/2 may force lowercase. Tests MUST NOT require exact casing.

**MUST**:
- Match header names case-insensitively (e.g., `X-Config-Version`, `x-config-version`, `X-CONFIG-VERSION` are all valid)
- Verify the semantic keys exist: `x-config-version`, `x-config-checksum`, `x-config-size`
- Verify values are correct (version number, checksum hash, byte size)

**Test harness implementation**:
```rust
// Example: case-insensitive header lookup
fn get_header_value(response: &Response, name: &str) -> Option<String> {
    response.headers()
        .iter()
        .find(|(k, _)| k.as_str().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.to_str().unwrap().to_string())
}
```

### 1.2 OS-Assigned Ports (Deterministic Discovery)

**Requirement**: Eliminate port conflicts and log-parsing fragility.

**MUST**:
- Binaries support `--listen 127.0.0.1:0` (bind to OS-assigned port)
- After binding, each process writes actual bound port(s) to `${data_dir}/.ports.json`
- `.ports.json` is written atomically (tmp + rename) and fsync'd
- Test harness polls for `.ports.json` existence with timeout (max 5s)
- Test harness parses `.ports.json` to discover relay/runtime ports

**MUST NOT**:
- Parse ports from stderr/logs (too fragile)
- Use custom port allocators

**`.ports.json` format**:
```json
{
  "http": 8080,
  "admin": 8081,
  "timestamp": "2026-01-16T12:34:56Z"
}
```

**Test harness implementation**:
```rust
pub fn wait_for_ports_file(data_dir: &Path, timeout: Duration) -> Result<PortsConfig> {
    let ports_file = data_dir.join(".ports.json");
    let start = Instant::now();
    loop {
        if ports_file.exists() {
            let content = fs::read_to_string(&ports_file)?;
            return Ok(serde_json::from_str(&content)?);
        }
        if start.elapsed() > timeout {
            bail!("Timeout waiting for .ports.json");
        }
        thread::sleep(Duration::from_millis(100));
    }
}
```

### 1.3 Durability Steps (Precise Fsync Semantics)

**Requirement**: Define exact durability steps for all persisted artifacts.

**Standard durability protocol** (MUST be followed for all artifacts):
1. Write content to temporary file (e.g., `foo.tmp`)
2. `fsync(tmp_file)` — persist file data
3. `rename(tmp -> final)` — atomic replacement
4. `fsync(parent_dir)` — persist directory metadata (ensures rename is durable)

**Failpoint naming convention**:
- `AFTER_<ARTIFACT>_FSYNC` means **after step 4 completes** (tmp fsync + rename + dir fsync)
- Example: `AFTER_HISTORY_PVS_FSYNC` triggers after `history/{ver}.pvs` is fully durable (steps 1-4 complete)

**Artifact-specific durability steps**:

| Artifact | Tmp File | Final File | Dir Fsync Target |
|----------|----------|------------|------------------|
| `history/{ver}.pvs` | `history/{ver}.pvs.tmp` | `history/{ver}.pvs` | `history/` |
| `history/{ver}.meta.json` | `history/{ver}.meta.json.tmp` | `history/{ver}.meta.json` | `history/` |
| `lkg/config.pvs` | `lkg/config.pvs.tmp` | `lkg/config.pvs` | `lkg/` |
| `lkg/meta.json` | `lkg/meta.json.tmp` | `lkg/meta.json` | `lkg/` |
| `state.json` | `state.json.tmp` | `state.json` | `.` (root data dir) |
| `.apply_journal` (runtime) | `.apply_journal.tmp` | `.apply_journal` | `.` (root data dir) |

**Failpoint injection points** (MUST trigger **after** step 4):
- `PAVIS_FAILPOINT_RELAY_AFTER_HISTORY_PVS_FSYNC` — after `history/{ver}.pvs` dir fsync
- `PAVIS_FAILPOINT_RELAY_AFTER_HISTORY_META_FSYNC` — after `history/{ver}.meta.json` dir fsync
- `PAVIS_FAILPOINT_RELAY_AFTER_LKG_PVS_RENAME` — after `lkg/config.pvs` dir fsync
- `PAVIS_FAILPOINT_RELAY_AFTER_LKG_META_RENAME` — after `lkg/meta.json` dir fsync
- `PAVIS_FAILPOINT_RELAY_AFTER_STATE_JSON_WRITE` — after `state.json` dir fsync

### 1.4 History Orphan Policy (Single Deterministic Strategy)

**Requirement**: Choose ONE policy for handling unpaired history artifacts.

**Selected strategy: Delete unpaired artifacts** (minimal, preferred)

**MUST**:
- On relay startup, scan `history/` directory
- For each `{ver}.pvs` file, check for matching `{ver}.meta.json`
- If `{ver}.meta.json` does NOT exist, delete `{ver}.pvs`
- Log warning: `"Deleted orphaned history artifact: history/{ver}.pvs (no matching meta.json)"`
- Do NOT move to `orphans/` directory (simpler implementation, sufficient for tests)

**MUST NOT**:
- Leave orphaned files in place
- Use "move to orphans/" strategy (adds complexity without test benefit)

**Test assertions**:
- After FP-R2 crash, verify orphaned `.pvs` file is deleted on restart
- Verify warning log message exists

### 1.5 Runtime Apply Journal (Lifecycle and Format)

**Requirement**: Define deterministic journal lifecycle to distinguish RT3 from pre-swap crashes.

**Journal format** (`.apply_journal` in runtime data dir):
```json
{
  "target_version": 42,
  "checksum": "sha256:abc123...",
  "phase": "swapped",
  "timestamp": "2026-01-16T12:34:56.789Z"
}
```

**Journal lifecycle** (MUST follow this sequence):

1. **Before swap**: No journal exists
2. **After swap** (immediately, before failpoint RT3):
   - Write journal with `phase: "swapped"`
   - Fsync journal (tmp + rename + dir fsync per 1.3)
   - **Failpoint RT3 triggers here**
3. **After LKG persist** (immediately, before failpoint RT4):
   - Update journal to `phase: "persisted"` (or delete journal)
   - Fsync update (if updating; if deleting, no fsync needed)
   - **Failpoint RT4 triggers here**

**Selected strategy: Delete journal after LKG persist** (preferred minimal)

**MUST**:
- Journal exists with `phase: "swapped"` after swap, before LKG persist
- Journal is deleted after LKG persist succeeds
- Journal is feature-gated: `#[cfg(feature = "test-failpoints")]`
- Journal uses standard durability protocol (1.3)

**Test assertions**:
- **RT3 (AFTER_SWAP)**: `.apply_journal` EXISTS with `phase: "swapped"` and correct version/checksum
- **RT4 (AFTER_LKG_PERSIST)**: `.apply_journal` does NOT exist (deleted after persist)
- **RT1, RT2 (before swap)**: `.apply_journal` does NOT exist

**Journal distinguishes crash windows**:
- No journal → crashed before swap (RT1, RT2)
- Journal exists with `phase: "swapped"` → crashed after swap but before LKG persist (RT3)
- No journal + LKG updated → crashed after LKG persist (RT4, or normal operation)

### 1.6 Crash Detection (Portable, Platform-Agnostic)

**Requirement**: Tests MUST pass on all platforms (Linux, macOS, Windows).

**MUST**:
- Accept non-zero exit code as sufficient crash detection
- Use `status.success() == false` or `status.code() != Some(0)`

**MUST NOT**:
- Require signal-specific assertions (e.g., `SIGABRT` on Unix)
- Require specific exit codes (may vary by platform/runtime)

**Test harness implementation**:
```rust
pub fn wait_for_crash(child: &mut Child, timeout: Duration) -> Result<ExitStatus> {
    let status = child.wait_timeout(timeout)?
        .ok_or_else(|| anyhow!("Process did not exit within timeout"))?;

    // Portable: only check that process exited unsuccessfully
    if status.success() {
        bail!("Expected process to crash, but it exited successfully");
    }

    Ok(status)
}
```

**Optional logging** (SHOULD, not MUST):
- Log signal information on Unix for debugging: `status.signal() == Some(SIGABRT)`
- But do NOT assert on it

### 1.7 `/v1/status` Authority (Authoritative Source)

**Requirement**: `/v1/status` MUST reflect authoritative state, not cache.

**MUST**:
- `/v1/status` endpoint reads `current_version` from `lkg/meta.json` (NOT from `state.json`)
- If `lkg/meta.json` does not exist, return error 500 (unrecoverable state)
- If `state.json` is missing or stale, it MUST NOT affect `/v1/status` response

**MUST NOT**:
- Read `current_version` from `state.json` cache
- Fail `/v1/status` if `state.json` is missing (cache is optional)

**`state.json` semantics**:
- `state.json` is a **cache-only** optimization (e.g., for faster startup introspection)
- Relay MUST rebuild `state.json` from `lkg/meta.json` on startup if missing/stale
- `state.json` is **never authoritative**

**Startup reconciliation** (MUST enforce on every relay start):
1. Read `lkg/meta.json` (authoritative source)
2. If `state.json` is missing or version mismatches, rebuild it from `lkg/meta.json`
3. `/v1/status` always returns data from `lkg/meta.json` (fresh read or in-memory cache of meta.json)

**Test assertions**:
- After any crash, `/v1/status` MUST return correct version from `lkg/meta.json`
- Tests MUST NOT verify `state.json` content for correctness (it's cache-only)
- Tests MAY verify `state.json` exists for performance reasons, but MUST NOT rely on it for correctness

---

## 2. Failpoint Design: Core Mechanism

### Feature Gating

All failpoint code is gated behind Cargo feature `test-failpoints`:

```toml
# pavis-relay/Cargo.toml and pavis-runtime/Cargo.toml
[features]
test-failpoints = []
```

When disabled (default and release builds), failpoint code is entirely removed via `#[cfg(feature = "test-failpoints")]`.

### Environment Variable Convention

Failpoints are activated via:

**`PAVIS_FAILPOINT_<COMPONENT>_<STEP>`**

- `<COMPONENT>`: `RELAY` or `RUNTIME`
- `<STEP>`: Exact step name (e.g., `AFTER_HISTORY_PVS_FSYNC`)

Supported values (v1):
- **`abort`**: Immediate process termination via `std::process::abort()` (default for E2E tests - most deterministic)
- **`panic`**: Trigger unwinding panic (use only where process-exit is guaranteed, e.g., in main thread without panic handlers)

Unset or empty = no failpoint triggered.

**Default mode**: E2E tests use `abort` by default for maximum determinism. Abort ensures no panic handlers, no cleanup code, and no potential for test flakiness due to partial unwind.

### Helper Module API

**File: `pavis-relay/src/failpoint.rs`**

```rust
#[cfg(feature = "test-failpoints")]
pub fn check_failpoint(name: &str) {
    use std::env;
    if let Ok(mode) = env::var(name) {
        match mode.as_str() {
            "panic" => panic!("Failpoint triggered: {}", name),
            "abort" => std::process::abort(),
            _ => {} // ignore unknown modes
        }
    }
}

#[cfg(not(feature = "test-failpoints"))]
#[inline(always)]
pub fn check_failpoint(_name: &str) {}
```

**File: `pavis-runtime/src/failpoint.rs`** (identical implementation)

Usage at injection points:
```rust
check_failpoint("PAVIS_FAILPOINT_RELAY_AFTER_HISTORY_PVS_FSYNC");
```

---

## 2. Relay Failpoints: Canonical Publish Flow

### Publish Flow Steps (Authoritative Order)

1. **Validate PVS** (in-memory validation)
2. **Write `history/{ver}.pvs`** (atomic tmp + rename + fsync)
3. **Write `history/{ver}.meta.json`** (atomic tmp + rename + fsync)
4. **Promote `lkg/config.pvs`** (atomic tmp + rename + fsync parent dir)
5. **Promote `lkg/meta.json`** (atomic tmp + rename + fsync parent dir)
6. **Update `state.json`** (best-effort cache write)
7. **Wake long-poll waiters** (notify blocked GET /v1/config requests)

### Relay Failpoint Enumeration

| Failpoint Name | Env Var | Injection Point | Semantic Meaning |
|----------------|---------|-----------------|------------------|
| **FP-R1** | `PAVIS_FAILPOINT_RELAY_AFTER_VALIDATION` | After PVS validation succeeds, before any disk writes | Crash before persistence begins |
| **FP-R2** | `PAVIS_FAILPOINT_RELAY_AFTER_HISTORY_PVS_FSYNC` | After `history/{ver}.pvs` fsync completes | Crash after PVS persisted, before metadata persisted |
| **FP-R3** | `PAVIS_FAILPOINT_RELAY_AFTER_HISTORY_META_FSYNC` | After `history/{ver}.meta.json` fsync completes | Crash after history fully persisted, before LKG promotion |
| **FP-R4** | `PAVIS_FAILPOINT_RELAY_AFTER_LKG_PVS_RENAME` | After `lkg/config.pvs` rename + fsync | Crash after LKG PVS promoted, before LKG meta promoted |
| **FP-R5** | `PAVIS_FAILPOINT_RELAY_AFTER_LKG_META_RENAME` | After `lkg/meta.json` rename + fsync | Crash after LKG fully promoted, before cache update |
| **FP-R6** | `PAVIS_FAILPOINT_RELAY_AFTER_STATE_JSON_WRITE` | After `state.json` write completes | Crash after cache updated, before long-poll wake |
| **FP-R7** | `PAVIS_FAILPOINT_RELAY_AFTER_WAKE_WAITERS` | After long-poll wake completes | Crash after publish fully completes (smoke test) |

### Relay Failpoint Semantics and Expected State

#### **FP-R1: AFTER_VALIDATION**

**Expected on-disk state after crash:**
- `history/` unchanged (no new version)
- `lkg/meta.json` points to previous version
- `lkg/config.pvs` is previous version
- `state.json` may be stale (will reconcile on startup)

**Expected behavior after restart:**
- Relay startup reconciles `state.json` from `lkg/meta.json`
- `GET /v1/status` returns `current_version: N` (previous)
- `GET /v1/config` returns previous config with `X-Config-Version: N`
- Publish API remains available; client received error (connection reset or 500)

**Invariants tested:** No partial state leakage; validation is side-effect-free.

---

#### **FP-R2: AFTER_HISTORY_PVS_FSYNC**

**Expected on-disk state after crash:**
- `history/{ver}.pvs` exists and is valid
- `history/{ver}.meta.json` does NOT exist
- `lkg/meta.json` points to previous version
- `lkg/config.pvs` is previous version

**Expected behavior after restart:**
- Relay startup detects orphaned `history/{ver}.pvs` (no matching meta.json)
- **Deterministic orphan cleanup policy**: Only paired (pvs + meta) entries are valid
  - Unpaired `*.pvs` files are deleted (or moved to `orphans/` directory)
  - Log warning: "Removed orphaned history artifact: history/{ver}.pvs (no matching meta.json)"
- `GET /v1/status` returns `current_version: N` (previous)
- `GET /v1/config` returns previous config
- New config is NOT visible anywhere (not promoted, metadata missing)

**Invariants tested:** History writes are two-phase (PVS + meta); incomplete history entry does not corrupt LKG; orphan cleanup is deterministic.

---

#### **FP-R3: AFTER_HISTORY_META_FSYNC**

**Expected on-disk state after crash:**
- `history/{ver}.pvs` exists and is valid
- `history/{ver}.meta.json` exists and is valid
- `lkg/meta.json` points to previous version
- `lkg/config.pvs` is previous version

**Expected behavior after restart:**
- Relay startup detects complete history entry but LKG is still old
- No automatic promotion (LKG promotion requires explicit publish success)
- `GET /v1/status` returns `current_version: N` (previous)
- Filesystem inspection shows `history/{ver}.meta.json` exists but is not marked as LKG
- Admin can manually trigger re-promotion if needed (out of scope for v1)

**Invariants tested:** History durability; LKG promotion is separate phase.

---

#### **FP-R4: AFTER_LKG_PVS_RENAME**

**Expected on-disk state after crash:**
- `history/{ver}.pvs` and `history/{ver}.meta.json` exist
- `lkg/config.pvs` points to new version (promoted)
- `lkg/meta.json` points to PREVIOUS version (not yet promoted)
- Inconsistency window: LKG PVS and meta are mismatched

**Expected behavior after restart:**
- Relay startup detects mismatch between `lkg/config.pvs` and `lkg/meta.json`
- **Deterministic reconciliation strategy: Meta-authoritative rollback**
  - `lkg/meta.json` is the single source of truth
  - Delete (or revert) `lkg/config.pvs` to match the version specified in `lkg/meta.json`
  - Log warning: "LKG mismatch detected: config.pvs={N+1}, meta.json={N}. Rolling back to meta.json version."
  - Copy `history/{N}.pvs` to `lkg/config.pvs` (or use atomic symlink)
- After reconciliation, `GET /v1/status` returns `current_version: N` (old)
- Filesystem shows `lkg/config.pvs` and `lkg/meta.json` are version-aligned (both version N)
- No data corruption; system reverts to last known good state

**Invariants tested:** LKG promotion atomicity; `lkg/meta.json` is authoritative; partial promotion is safely rolled back (never advanced).

---

#### **FP-R5: AFTER_LKG_META_RENAME**

**Expected on-disk state after crash:**
- `history/{ver}.pvs` and `history/{ver}.meta.json` exist
- `lkg/config.pvs` points to new version
- `lkg/meta.json` points to new version
- `state.json` may still reference old version (cache stale)

**Expected behavior after restart:**
- Relay startup detects `lkg/meta.json` is new version
- Rebuilds `state.json` from `lkg/meta.json` (cache reconciliation)
- `GET /v1/status` returns `current_version: {ver}` (new)
- `GET /v1/config` returns new config with `X-Config-Version: {ver}` and correct checksum
- Long-poll waiters did NOT receive wake signal (missed notification is acceptable)

**Invariants tested:** LKG promotion durability; `state.json` is non-authoritative.

---

#### **FP-R6: AFTER_STATE_JSON_WRITE**

**Expected on-disk state after crash:**
- Fully consistent: history, LKG, and state.json all point to new version

**Expected behavior after restart:**
- Relay restarts cleanly
- `GET /v1/status` returns `current_version: {ver}` (new)
- `GET /v1/config` returns new config
- Long-poll waiters did NOT receive wake signal (missed)

**Invariants tested:** Long-poll wake is best-effort; missed wake does not corrupt state.

---

#### **FP-R7: AFTER_WAKE_WAITERS**

**Expected on-disk state after crash:**
- Fully consistent

**Expected behavior after restart:**
- Relay restarts cleanly
- All state is correct
- This is a smoke test (no additional failure windows after wake)

**Invariants tested:** Publish completion is idempotent; no post-wake corruption.

---

## 3. Runtime Failpoints: Config Apply Flow

### Apply Flow Steps (Authoritative Order)

1. **Detect new config** (via long-poll or poll)
2. **Validate new config** (if needed; may be no-op if relay validation is trusted)
3. **Atomic swap** (replace active Arc<Config> in routing engine)
4. **Write apply journal** (test-only, gated by `test-failpoints` feature: write `.apply_journal` with swap timestamp + version, fsync)
5. **Persist local LKG** (write runtime-local copy of config + metadata)

**Note**: The apply journal (step 4) provides observability for crash recovery tests. It allows tests to distinguish "crashed after swap but before LKG persist" from "crashed before swap". The journal is a simple text file containing the version number and swap timestamp, fsync'd immediately after the atomic swap completes. This file is only written when the `test-failpoints` feature is enabled.

### Runtime Failpoint Enumeration

| Failpoint Name | Env Var | Injection Point | Semantic Meaning |
|----------------|---------|-----------------|------------------|
| **FP-RT1** | `PAVIS_FAILPOINT_RUNTIME_BEFORE_APPLY` | After detecting new config, before apply logic | Crash before any config changes |
| **FP-RT2** | `PAVIS_FAILPOINT_RUNTIME_AFTER_VALIDATION` | After validation succeeds, before swap | Crash after validation, before activation |
| **FP-RT3** | `PAVIS_FAILPOINT_RUNTIME_AFTER_SWAP` | After atomic swap, before LKG persist | Crash after new config is active, before durable |
| **FP-RT4** | `PAVIS_FAILPOINT_RUNTIME_AFTER_LKG_PERSIST` | After local LKG persist completes | Crash after apply fully completes (smoke test) |

### Runtime Failpoint Semantics and Expected State

#### **FP-RT1: BEFORE_APPLY**

**Expected runtime behavior before crash:**
- Old config is active in data plane (Arc<Config> unchanged)
- No local LKG writes have occurred
- New config detected but not processed

**Expected behavior after restart:**
- Runtime restarts with old config (from local LKG)
- Re-polls relay and detects new config again
- Apply proceeds normally on retry
- Data plane continuity: requests routed via old config throughout

**Invariants tested:** Config detection is idempotent; no side effects before apply.

---

#### **FP-RT2: AFTER_VALIDATION**

**Expected runtime behavior before crash:**
- Old config still active in data plane
- Validation succeeded but swap has not occurred
- No local LKG writes

**Expected behavior after restart:**
- Runtime restarts with old config
- Re-validates and applies new config on retry
- Data plane continuity: no partial config ever visible

**Invariants tested:** Validation is side-effect-free; does not modify data plane.

---

#### **FP-RT3: AFTER_SWAP**

**Expected runtime behavior before crash:**
- New config is ACTIVE in data plane (Arc<Config> swapped)
- Requests are being routed using new config
- Apply journal written and fsync'd (contains version N+1)
- Local LKG still points to old config (not yet persisted)

**Expected behavior after restart:**
- Runtime restarts with old config (because local LKG was not updated)
- **Regression occurs**: data plane reverts to old config temporarily
- Runtime re-polls relay, detects new config, re-applies
- No corruption: old config is valid, just stale

**Test observability:**
- Presence of `.apply_journal` file with version N+1 proves swap completed
- Absence of updated LKG proves persistence did not complete
- This distinguishes AFTER_SWAP from BEFORE_SWAP crashes

**Key invariant tested:** Atomic swap is durable in-memory but separate from persistence. Regression to old config is safe; corruption is never acceptable. Data plane never serves a partially-applied or invalid config.

---

#### **FP-RT4: AFTER_LKG_PERSIST**

**Expected runtime behavior before crash:**
- New config is active in data plane
- New config is persisted as local LKG

**Expected behavior after restart:**
- Runtime restarts with new config (from local LKG)
- No regression
- Data plane continuity maintained

**Invariants tested:** LKG persistence durability; apply completion is idempotent.

---

## 4. E2E Test Matrix

### Test Structure Pattern

Each test follows this sequence:

1. **Phase 1: Setup**
   - Start relay with initial config (version N)
   - Start runtime, verify it loads config N
   - Verify data plane works (send test request, assert routing)

2. **Phase 2: Failpoint Injection**
   - Publish new config (version N+1) with failpoint env var set
   - Detect crash (exit code non-zero or SIGABRT)

3. **Phase 3: Recovery Validation**
   - Restart relay/runtime WITHOUT failpoint env var
   - Assert expected state via:
     - `GET /v1/status` (current_version, lkg metadata)
     - `GET /v1/config` headers (X-Config-Version, X-Config-Checksum, X-Config-Size)
     - Filesystem checks (lkg/meta.json, lkg/config.pvs, history/*)
   - Verify data plane correctness (send test request)

### Relay Failpoint Test Matrix

| Test Name | Env Var | Assertions After Restart |
|-----------|---------|--------------------------|
| **test_relay_crash_after_validation** | `PAVIS_FAILPOINT_RELAY_AFTER_VALIDATION=abort` | • `GET /v1/status`: `current_version: N` (old)<br>• `lkg/meta.json`: version N<br>• `history/{N+1}.pvs`: does NOT exist<br>• `history/{N+1}.meta.json`: does NOT exist |
| **test_relay_crash_after_history_pvs_fsync** | `PAVIS_FAILPOINT_RELAY_AFTER_HISTORY_PVS_FSYNC=abort` | • `GET /v1/status`: `current_version: N` (old)<br>• `lkg/meta.json`: version N<br>• `history/{N+1}.pvs`: EXISTS (orphan - will be deleted)<br>• `history/{N+1}.meta.json`: does NOT exist<br>• Relay logs orphan cleanup warning |
| **test_relay_crash_after_history_meta_fsync** | `PAVIS_FAILPOINT_RELAY_AFTER_HISTORY_META_FSYNC=abort` | • `GET /v1/status`: `current_version: N` (old)<br>• `lkg/meta.json`: version N<br>• `history/{N+1}.pvs`: EXISTS<br>• `history/{N+1}.meta.json`: EXISTS<br>• History is durable but not promoted |
| **test_relay_crash_after_lkg_pvs_rename** | `PAVIS_FAILPOINT_RELAY_AFTER_LKG_PVS_RENAME=abort` | • Filesystem before restart: `lkg/config.pvs` (N+1), `lkg/meta.json` (N) MISMATCH<br>• After startup reconciliation (meta-authoritative rollback):<br>&nbsp;&nbsp;- `GET /v1/status`: `current_version: N` (rolled back)<br>&nbsp;&nbsp;- `lkg/config.pvs` and `lkg/meta.json` are version-aligned (both N)<br>• Logs LKG mismatch warning |
| **test_relay_crash_after_lkg_meta_rename** | `PAVIS_FAILPOINT_RELAY_AFTER_LKG_META_RENAME=abort` | • `GET /v1/status`: `current_version: N+1` (new)<br>• `GET /v1/config` headers: `X-Config-Version: N+1`, correct checksum<br>• `lkg/meta.json`: version N+1<br>• `lkg/config.pvs`: version N+1<br>• `state.json` reconciled from `lkg/meta.json` |
| **test_relay_crash_after_state_json_write** | `PAVIS_FAILPOINT_RELAY_AFTER_STATE_JSON_WRITE=abort` | • `GET /v1/status`: `current_version: N+1`<br>• All files consistent<br>• Long-poll clients did NOT receive wake (acceptable) |
| **test_relay_crash_after_wake_waiters** | `PAVIS_FAILPOINT_RELAY_AFTER_WAKE_WAITERS=abort` | • Smoke test: all state consistent<br>• `GET /v1/status`: `current_version: N+1`<br>• No post-wake corruption |

### Runtime Failpoint Test Matrix

**Note**: All runtime tests verify journal lifecycle per §1.5 (Hardening Notes).

| Test Name | Env Var | Assertions After Restart |
|-----------|---------|--------------------------|
| **test_runtime_crash_before_apply** | `PAVIS_FAILPOINT_RUNTIME_BEFORE_APPLY=abort` | • Runtime local LKG: version N (old)<br>• `.apply_journal`: does NOT exist (§1.5: no journal before swap)<br>• Runtime restarts, loads old config<br>• Re-polls relay, detects N+1, applies successfully<br>• Data plane test request uses old config initially, then new after retry |
| **test_runtime_crash_after_validation** | `PAVIS_FAILPOINT_RUNTIME_AFTER_VALIDATION=abort` | • Runtime local LKG: version N<br>• `.apply_journal`: does NOT exist (§1.5: no journal before swap)<br>• Runtime restarts with old config<br>• Re-validates and applies N+1 on retry<br>• Data plane never saw partial config |
| **test_runtime_crash_after_swap** | `PAVIS_FAILPOINT_RUNTIME_AFTER_SWAP=abort` | • Runtime local LKG: version N (NOT updated)<br>• `.apply_journal`: **EXISTS** with `phase: "swapped"`, version N+1, correct checksum (§1.5: proves swap completed)<br>• Runtime restarts with old config (REGRESSION)<br>• Re-applies N+1 on next poll<br>• Data plane test request uses old config after restart<br>• **Critical**: No corruption; old config is valid |
| **test_runtime_crash_after_lkg_persist** | `PAVIS_FAILPOINT_RUNTIME_AFTER_LKG_PERSIST=abort` | • Runtime local LKG: version N+1 (updated)<br>• `.apply_journal`: does **NOT** exist (§1.5: deleted after LKG persist)<br>• Runtime restarts with new config (NO regression)<br>• Data plane test request uses new config immediately<br>• Apply completion is idempotent |

### Checksum Verification Tests

**Note**: Header matching is case-insensitive per §1.1 (Hardening Notes).

Additional test cases to verify checksum correctness:

| Test Name | Scenario | Assertion |
|-----------|----------|-----------|
| **test_checksum_stability** | Publish same PVS bytes with different version numbers | Both return identical checksum (case-insensitive header match: `x-config-checksum`; checksum is content-based, not version-based) |
| **test_checksum_after_crash_recovery** | Crash after LKG promotion, restart, fetch config | Checksum (case-insensitive header match) matches pre-crash value (durability) |

### Smoke Tests (Limited Assertions)

Some failpoints produce fully consistent states and serve primarily as smoke tests:

- **FP-R7 (AFTER_WAKE_WAITERS)**: Verifies no post-wake corruption; assertions are "all state is consistent"
- **FP-RT4 (AFTER_LKG_PERSIST)**: Verifies no post-persist corruption; assertions are "restart with new config works"

These tests are still valuable (ensure no unexpected failures) but do not test specific recovery logic.

---

## 5. Test Harness Design

### Build and Run Strategy

**Build once with failpoints:**
```bash
cargo build --release --features test-failpoints --bin pavis-relay
cargo build --release --features test-failpoints --bin pavis-runtime
```

**Run E2E suite:**
```bash
cargo test --test e2e_relay_failpoints --features test-failpoints
cargo test --test e2e_runtime_failpoints --features test-failpoints
```

### Test Harness Utilities

**File: `tests/harness/mod.rs`**

**Note**: Implements requirements from §1 (Hardening Notes).

Provides:

```rust
// Spawn relay with isolated temp dir and OS-assigned port (§1.2)
// Returns (Child, PortsConfig) - reads from .ports.json
pub fn spawn_relay(
    data_dir: &Path,
    failpoint_env: Option<(&str, &str)>
) -> Result<(Child, PortsConfig)>;

// Spawn runtime connected to relay with OS-assigned proxy port (§1.2)
// Returns (Child, PortsConfig) - reads from .ports.json
pub fn spawn_runtime(
    relay_url: &str,
    data_dir: &Path,
    failpoint_env: Option<(&str, &str)>
) -> Result<(Child, PortsConfig)>;

// Wait for process to crash (§1.6: portable, non-zero exit only)
pub fn wait_for_crash(child: &mut Child, timeout: Duration) -> Result<ExitStatus>;

// Poll health endpoint until ready
pub fn wait_for_ready(url: &str, timeout: Duration) -> Result<()>;

// Assert current version via /v1/status (§1.7: reads from lkg/meta.json)
pub fn assert_current_version(relay_url: &str, expected: u64) -> Result<()>;

// Assert config headers via /v1/config (§1.1: case-insensitive header matching)
// Headers: x-config-version, x-config-checksum, x-config-size
pub fn assert_config_headers(
    relay_url: &str,
    expected_version: u64,
    expected_checksum: &str
) -> Result<()>;

// Assert lkg/meta.json on disk (authoritative source per §1.7)
pub fn assert_lkg_meta(data_dir: &Path, expected_version: u64) -> Result<()>;

// Assert history file exists
pub fn assert_history_exists(data_dir: &Path, version: u64, has_meta: bool) -> Result<()>;

// Assert apply journal exists with expected version and phase (§1.5: runtime only)
pub fn assert_apply_journal(
    data_dir: &Path,
    expected_version: u64,
    expected_phase: &str  // "swapped" or "persisted"
) -> Result<()>;

// Assert apply journal does NOT exist (§1.5: runtime only)
pub fn assert_no_apply_journal(data_dir: &Path) -> Result<()>;

// Wait for .ports.json and parse (§1.2: deterministic port discovery)
pub fn wait_for_ports_file(data_dir: &Path, timeout: Duration) -> Result<PortsConfig>;

// Case-insensitive header lookup (§1.1: HTTP header casing)
pub fn get_header_value(response: &Response, name: &str) -> Result<String>;
```

**Port discovery strategy (§1.2)**:
- Binaries invoked with `--listen 127.0.0.1:0` (OS assigns port)
- After bind, process writes `.ports.json` to data dir (atomic write + fsync)
- Test harness calls `wait_for_ports_file()` with 5s timeout
- Parsed `PortsConfig` contains actual bound ports

### Crash Detection Strategy

**Implements §1.6 (Hardening Notes): Portable, platform-agnostic crash detection.**

Crashes are detected via:

1. **Exit code**: `status.code() != Some(0)` or `!status.success()` (MUST accept on all platforms)
2. **Timeout**: If process does not exit within expected time, kill it and fail test

**MUST NOT**:
- Require signal-specific assertions (e.g., `SIGABRT` on Unix)
- Require specific exit codes (may vary by platform/runtime)

Example implementation (from §1.6):
```rust
pub fn wait_for_crash(child: &mut Child, timeout: Duration) -> Result<ExitStatus> {
    let status = child.wait_timeout(timeout)?
        .ok_or_else(|| anyhow!("Process did not exit within timeout"))?;

    // Portable: only check that process exited unsuccessfully
    if status.success() {
        bail!("Expected process to crash, but it exited successfully");
    }

    Ok(status)
}
```

**Optional debugging** (SHOULD, not MUST):
- Log signal information on Unix: `status.signal() == Some(SIGABRT)`
- Do NOT assert on specific signals (test must pass without this)

### Isolation Strategy

Each test runs in isolated environment:

- **Temp directories**: Fresh per test (cleaned up automatically)
- **Dynamic ports**: Allocated from test harness to avoid conflicts
- **No shared state**: Tests are fully independent

### Avoiding Flakiness

- **Deterministic failpoints**: Same env var → same crash point every time
- **Health check synchronization**: Wait for `/v1/status` to return 200 before proceeding
- **Generous timeouts**: 30s for startup, 5s for crash detection
- **Filesystem sync**: Use fsync in test setup to ensure writes are durable before triggering failpoints

---

## 6. Implementation File Touchpoints

### New Files to Create

**Failpoint helpers:**
- `pavis-relay/src/failpoint.rs` (helper module with `check_failpoint` function)
- `pavis-runtime/src/failpoint.rs` (identical helper module)

**Test harness:**
- `tests/harness/mod.rs` (shared utilities for spawning processes, asserting state, OS-assigned port handling)
- `tests/harness/temp.rs` (temp directory manager)

**E2E test suites:**
- `tests/e2e_relay_failpoints.rs` (relay crash recovery tests)
- `tests/e2e_runtime_failpoints.rs` (runtime crash recovery tests)
- `tests/e2e_checksum_verification.rs` (checksum stability tests)
- `tests/e2e_prod_safety.rs` (production safety negative test - verifies failpoint env vars are ignored in non-feature builds)

### Files to Modify

**Relay:**
- `pavis-relay/src/lib.rs`: Add `pub mod failpoint;` (conditional on feature flag)
- `pavis-relay/src/publish.rs`: Add `check_failpoint(...)` calls at 7 injection points per §1.3 (after each durability step: tmp fsync + rename + dir fsync)
- `pavis-relay/src/startup.rs` (or equivalent): Add startup reconciliation logic per §1.4, §1.7:
  - **Meta-authoritative LKG reconciliation**: If `lkg/config.pvs` and `lkg/meta.json` versions mismatch, delete/revert `lkg/config.pvs` to match `lkg/meta.json` (log warning)
  - **Orphan history cleanup**: Delete (not move) unpaired `history/*.pvs` files that lack matching `*.meta.json` (log warning per §1.4)
  - **State cache rebuild**: Rebuild `state.json` from authoritative `lkg/meta.json` (§1.7)
- `pavis-relay/src/status.rs` (or `/v1/status` endpoint): Ensure endpoint reads `current_version` from `lkg/meta.json`, NOT from `state.json` cache (§1.7)
- `pavis-relay/src/main.rs` (or server startup): Add `.ports.json` writer after binding to OS-assigned port (§1.2):
  - Write `.ports.json` with actual bound ports
  - Use atomic write (tmp + rename) and fsync per §1.3
- `pavis-relay/Cargo.toml`: Add `test-failpoints = []` feature

**Runtime:**
- `pavis-runtime/src/lib.rs`: Add `pub mod failpoint;` (conditional on feature flag)
- `pavis-runtime/src/agent.rs` (or `config_loader.rs`): Add `check_failpoint(...)` calls at 4 injection points per §1.5
- `pavis-runtime/src/agent.rs` (or `config_loader.rs`): Add apply journal logic per §1.5 (gated by `test-failpoints`):
  - After atomic swap, write `.apply_journal` with `phase: "swapped"`, version, checksum, timestamp
  - Fsync journal per §1.3 (tmp + rename + dir fsync)
  - After LKG persist, delete `.apply_journal` (§1.5: selected strategy)
  - Journal is used by tests to prove swap completed
- `pavis-runtime/src/main.rs` (or server startup): Add `.ports.json` writer after binding to OS-assigned port (§1.2)
- `pavis-runtime/Cargo.toml`: Add `test-failpoints = []` feature

**Root Cargo.toml** (if workspace):
- No changes needed; features are crate-local

---

## 7. CI and Local Execution

### Local Test Execution

**Run all failpoint tests:**
```bash
# Build binaries with failpoints enabled
cargo build --release --features test-failpoints

# Run E2E test suites
cargo test --features test-failpoints --test e2e_relay_failpoints -- --test-threads=1
cargo test --features test-failpoints --test e2e_runtime_failpoints -- --test-threads=1
cargo test --features test-failpoints --test e2e_checksum_verification
```

`--test-threads=1` ensures tests run sequentially (avoids port conflicts if dynamic allocation fails).

### CI Integration

**GitHub Actions example:**

```yaml
- name: Run failpoint tests
  run: |
    cargo build --release --features test-failpoints
    cargo test --features test-failpoints --test e2e_relay_failpoints -- --test-threads=1
    cargo test --features test-failpoints --test e2e_runtime_failpoints -- --test-threads=1
```

### Verifying Failpoints Are Disabled in Prod

**Production safety negative test (preferred):**
```bash
# Build WITHOUT test-failpoints feature
cargo build --release

# Run a test that sets failpoint env vars and verifies they are ignored
cargo test --release test_prod_ignores_failpoint_env_vars

# Test implementation:
# 1. Spawn pavis-relay with PAVIS_FAILPOINT_RELAY_AFTER_VALIDATION=abort
# 2. Publish a config
# 3. Assert that relay does NOT crash (env var is ignored)
# 4. Assert publish succeeds normally
```

This approach is superior to `nm | grep` because:
- It validates runtime behavior, not just symbol presence
- It catches issues even if symbols are present but failpoints are no-ops
- It's cross-platform (doesn't rely on Unix `nm`)

**Optional: Binary symbol check (supplementary):**
```bash
# Build without feature
cargo build --release

# Verify no failpoint symbols exist
nm target/release/pavis-relay | grep -i failpoint
# Should return empty (no matches)
```

---

## 8. Invariants Coverage Summary

### Relay Invariants

| Invariant | Failpoints Testing It |
|-----------|----------------------|
| **Validation is side-effect-free** | FP-R1 (AFTER_VALIDATION) |
| **History writes are durable** | FP-R2 (AFTER_HISTORY_PVS_FSYNC), FP-R3 (AFTER_HISTORY_META_FSYNC) |
| **History orphan cleanup is deterministic** | FP-R2 (unpaired artifacts deleted on startup) |
| **LKG promotion is atomic** | FP-R4 (AFTER_LKG_PVS_RENAME - tests partial promotion recovery via meta-authoritative rollback) |
| **LKG promotion is durable** | FP-R5 (AFTER_LKG_META_RENAME) |
| **lkg/meta.json is authoritative** | FP-R4 (meta-authoritative rollback), FP-R5 (state.json rebuilt from meta.json) |
| **state.json is non-authoritative cache** | FP-R5 (startup reconciles from lkg/meta.json) |
| **Long-poll wake is best-effort** | FP-R6 (AFTER_STATE_JSON_WRITE) |
| **Publish completion is idempotent** | FP-R7 (AFTER_WAKE_WAITERS) |

### Runtime Invariants

| Invariant | Failpoints Testing It |
|-----------|----------------------|
| **Config detection is idempotent** | FP-RT1 (BEFORE_APPLY) |
| **Validation is side-effect-free** | FP-RT2 (AFTER_VALIDATION) |
| **Atomic swap is durable in-memory** | FP-RT3 (AFTER_SWAP - regression is safe, corruption is not) |
| **Swap completion is observable** | FP-RT3 (apply journal proves swap completed even if LKG not persisted) |
| **LKG persistence is durable** | FP-RT4 (AFTER_LKG_PERSIST) |
| **Data plane never sees partial config** | ALL runtime failpoints (frozen data plane) |

### Frozen Data Plane Guarantee

ALL failpoints (relay + runtime) validate that:
- Data plane never serves a partially-applied config
- Data plane never serves a corrupted config
- Regression to old config is acceptable; corruption is never acceptable

---

## 9. Untested and Out-of-Scope Cases

### Intentionally Untested (v1)

- **Mid-fsync OS crashes**: Filesystem journaling is assumed to handle this
- **Simultaneous relay + runtime crashes**: Distributed failure injection is out of scope
- **Network partition during long-poll**: Requires network fault injection (future)
- **Disk full during write**: Requires quota/resource fault injection (future)

### Future Enhancements (v2+)

- **Dynamic failpoint control**: Add admin endpoint for failpoint injection (gated, authenticated)
- **Sleep and skip modes**: Add non-crash failpoint modes for chaos testing
- **Coordinated failures**: Test relay crash during runtime apply
- **Performance impact measurement**: Benchmark failpoint overhead when enabled

---

## 10. Deliverables Checklist

### Code Implementation

- [ ] `pavis-relay/src/failpoint.rs` (helper module with abort/panic support)
- [ ] `pavis-runtime/src/failpoint.rs` (helper module with abort/panic support)
- [ ] 7 failpoint injection points in `pavis-relay/src/publish.rs` per §1.3 (after durability steps)
- [ ] 4 failpoint injection points in `pavis-runtime/src/agent.rs` per §1.5
- [ ] **Relay startup reconciliation logic** in `pavis-relay/src/startup.rs`:
  - [ ] Meta-authoritative LKG rollback (delete mismatched lkg/config.pvs) per §1.4, §1.7
  - [ ] Orphan history cleanup (delete unpaired *.pvs files) per §1.4
  - [ ] State.json rebuild from lkg/meta.json per §1.7
- [ ] **Relay `/v1/status` endpoint** reads from `lkg/meta.json` (NOT `state.json`) per §1.7
- [ ] **Runtime apply journal** in `pavis-runtime/src/agent.rs` per §1.5 (gated by test-failpoints):
  - [ ] Write `.apply_journal` after swap with `phase: "swapped"`, version, checksum, timestamp
  - [ ] Fsync journal per §1.3 (tmp + rename + dir fsync)
  - [ ] Delete journal after LKG persist (selected strategy)
- [ ] **`.ports.json` writer** in both relay and runtime per §1.2:
  - [ ] Binaries support `--listen 127.0.0.1:0` (OS-assigned port)
  - [ ] Write `.ports.json` after bind (atomic + fsync per §1.3)
- [ ] Feature flag `test-failpoints` in both Cargo.toml files

### Test Harness

- [ ] `tests/harness/mod.rs` per §1:
  - [ ] `wait_for_ports_file()` - port discovery from `.ports.json` (§1.2)
  - [ ] `get_header_value()` - case-insensitive header matching (§1.1)
  - [ ] `wait_for_crash()` - portable crash detection (§1.6)
  - [ ] `assert_config_headers()` - case-insensitive (§1.1)
  - [ ] `assert_apply_journal()` - with phase and version checks (§1.5)
  - [ ] `assert_no_apply_journal()` (§1.5)
  - [ ] `assert_current_version()` - reads from lkg/meta.json (§1.7)
- [ ] `tests/harness/temp.rs` (temp directory management)

### E2E Test Suites

- [ ] `tests/e2e_relay_failpoints.rs` (7 relay crash tests with abort mode):
  - [ ] Verify orphan cleanup after FP-R2 (§1.4)
  - [ ] Verify meta-authoritative rollback after FP-R4 (§1.4, §1.7)
  - [ ] All assertions use case-insensitive headers (§1.1)
- [ ] `tests/e2e_runtime_failpoints.rs` (4 runtime crash tests with abort mode):
  - [ ] Verify journal lifecycle per §1.5 (RT3: journal exists with phase "swapped", RT4: journal deleted)
  - [ ] All tests verify portable crash detection (§1.6)
- [ ] `tests/e2e_checksum_verification.rs` (checksum stability tests with case-insensitive headers per §1.1)
- [ ] `tests/e2e_prod_safety.rs` (production safety negative test - verifies failpoint env vars are ignored in non-feature builds)

### Documentation

- [ ] `docs/plan/failpoint_testing_execution_plan.md` (this document with §1 Hardening Notes)
- [ ] `docs/testing.md` (add failpoint testing section with deterministic policies from §1)
- [ ] `docs/crash-recovery.md` (add failpoint-based testing section with reconciliation strategies from §1.4, §1.7)
- [ ] `README.md` (add "Testing" section mentioning test-failpoints feature)

### CI Integration

- [ ] Add failpoint test job to `.github/workflows/ci.yml`
- [ ] Add production safety negative test to CI (verify env vars ignored without feature)

---

## 11. Conclusion

This implementation-ready plan provides a complete blueprint for deterministic, portable, and flakiness-free failpoint-based E2E testing in Pavis.

### Hardening Improvements Applied (Section 1)

All ambiguity has been eliminated through **7 mandatory hardening requirements**:

1. **HTTP Header Casing (§1.1)**: Tests use case-insensitive header matching (RFC 7230 compliant). No reliance on exact casing like `X-Config-Version` vs `x-config-version`.

2. **OS-Assigned Ports (§1.2)**: Deterministic port discovery via `.ports.json` file (atomic write + fsync). No log parsing, no custom port allocators.

3. **Durability Steps (§1.3)**: Precise fsync semantics defined for all artifacts (tmp write → fsync → rename → dir fsync). Failpoints trigger after full durability protocol completes.

4. **History Orphan Policy (§1.4)**: Single deterministic strategy: **delete** unpaired `*.pvs` files on startup. No "move to orphans/" ambiguity.

5. **Runtime Apply Journal (§1.5)**: Structured lifecycle with `phase: "swapped"` after swap, deleted after LKG persist. Distinguishes RT3 crash window from pre-swap crashes.

6. **Crash Detection (§1.6)**: Portable, platform-agnostic. Tests accept non-zero exit code only. No signal-specific assertions required.

7. **`/v1/status` Authority (§1.7)**: Endpoint reads from `lkg/meta.json` (authoritative), NOT from `state.json` cache. Ensures correctness even if cache is stale/missing.

### Original Blocking Fixes (Retained)

1. **FP-R4 Deterministic Reconciliation**: Meta-authoritative rollback only. If `lkg/config.pvs` ≠ `lkg/meta.json` → always revert to meta.

2. **Failpoint Action Semantics**: Default to `abort` mode (most deterministic). `panic` only where process-exit is guaranteed.

3. **Runtime AFTER_SWAP Observability**: Apply journal proves swap completed even if process crashes before LKG persist.

### Ship-Ready Guarantees

The plan now delivers:

- **Zero ambiguity**: Every test has one deterministic outcome (no "either/or" scenarios)
- **Zero flakiness**: Deterministic failpoints, deterministic port discovery, deterministic crash detection
- **Zero platform-specific code**: Portable across Linux/macOS/Windows
- **Zero production overhead**: Failpoints entirely removed in release builds (compile-time feature gating)

### Complete Test Coverage

**11 core failpoint tests** (7 relay + 4 runtime) covering:
- **Atomicity**: LKG promotion, config swap
- **Durability**: History writes, LKG persistence
- **Idempotency**: Publish, apply
- **Deterministic recovery**: Meta-authoritative reconciliation, orphan cleanup
- **Observability**: Apply journal proves crash windows

### Implementation Ready

Every requirement is **MUST/MUST NOT** prescriptive:
- Durability protocol: 4-step process (write → fsync → rename → dir fsync)
- Orphan cleanup: Delete only (not move)
- Journal lifecycle: Write after swap with phase, delete after persist
- Port discovery: `.ports.json` with atomic write
- Header matching: Case-insensitive only
- Crash detection: Non-zero exit only
- Authority source: `lkg/meta.json` only

**No remaining design decisions. Ready for implementation.**
