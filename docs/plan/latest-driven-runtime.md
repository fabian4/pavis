# Implementation Plan: Latest-Driven Runtime with ETag-Based Failure Deduplication

## 1. State Model Changes

### File: `crates/pavis/src/agent/worker/agent.rs`

**Current state fields (lines 26-36):**
```rust
pub struct ConfigAgent {
    relay_base: String,
    lkg_path: PathBuf,
    client: Client,
    backoff: Backoff,
    state: Arc<RuntimeStateHandle>,
    last_checksum: Arc<Mutex<Option<String>>>,      // ← Rename to last_applied_etag
    last_version: Arc<Mutex<Option<u64>>>,          // ← Remove (advisory only)
    on_update_callback: Mutex<Option<UpdateCallback>>,
    metrics: Arc<Mutex<Option<Arc<MetricsHandle>>>>,
}
```

**Changes:**

1. **Rename** `last_checksum` → `last_applied_etag` (semantics unchanged; clarify intent).
2. **Remove** `last_version` field entirely.
3. **Add** `last_rejected_etag: Arc<Mutex<Option<String>>>`.

**New struct (lines 26-36):**
```rust
pub struct ConfigAgent {
    relay_base: String,
    lkg_path: PathBuf,
    client: Client,
    backoff: Backoff,
    state: Arc<RuntimeStateHandle>,
    last_applied_etag: Arc<Mutex<Option<String>>>,
    last_rejected_etag: Arc<Mutex<Option<String>>>,
    on_update_callback: Mutex<Option<UpdateCallback>>,
    metrics: Arc<Mutex<Option<Arc<MetricsHandle>>>>,
}
```

**Constructor updates:**

- `new()` (lines 78-100): Initialize `last_rejected_etag: Arc::new(Mutex::new(None))`; remove `last_version` initialization.
- `new_for_tests()` (lines 131-149): Same changes.

**Accessor methods:**

- **Remove** (lines 465-479):
  - `fn last_version(&self) -> Option<u64>`
  - `fn set_last_version(&self, version: u64)`

- **Rename** (lines 449-463):
  - `fn is_checksum_current(&self, checksum: &str) -> bool` → `fn is_etag_current(&self, etag: &str) -> bool`
  - `fn set_last_checksum(&self, checksum: String)` → `fn set_last_applied_etag(&self, etag: String)`

- **Add** (new methods):
```rust
fn get_conditional_etag(&self) -> Option<String> {
    // Prefer rejected ETag for conditional requests
    let rejected = self.last_rejected_etag.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if rejected.is_some() {
        return rejected.clone();
    }
    let applied = self.last_applied_etag.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    applied.clone()
}

fn is_etag_rejected(&self, etag: &str) -> bool {
    let guard = self.last_rejected_etag.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.as_deref() == Some(etag)
}

fn set_last_rejected_etag(&self, etag: String) {
    let mut guard = self.last_rejected_etag.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(etag);
}

fn clear_last_rejected_etag(&self) {
    let mut guard = self.last_rejected_etag.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = None;
}
```

**Test-only methods:**

- **Remove** (lines 374-380):
  - `pub(crate) fn last_version_for_tests(&self) -> Option<u64>`

- **Rename** (lines 365-371, 383-389):
  - `last_checksum_for_tests()` → `last_applied_etag_for_tests()`
  - `set_last_checksum_for_tests()` → `set_last_applied_etag_for_tests()`

- **Add**:
```rust
#[cfg(test)]
pub(crate) fn last_rejected_etag_for_tests(&self) -> Option<String> {
    let guard = self.last_rejected_etag.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.clone()
}

#[cfg(test)]
pub(crate) fn set_last_rejected_etag_for_tests(&self, value: Option<String>) {
    let mut guard = self.last_rejected_etag.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = value;
}
```

---

## 2. Polling Control Flow

### File: `crates/pavis/src/agent/worker/agent.rs`

**Enum:** `PollOutcome` (lines 392-396)

**Current:**
```rust
#[derive(Debug)]
pub enum PollOutcome {
    Updated,
    NoChange,
}
```

**New:**
```rust
#[derive(Debug)]
pub enum PollOutcome {
    Updated,   // Successfully applied new configuration
    NoChange,  // No new configuration (304/204 or ETag matches current/rejected)
    Rejected,  // Validation failed; LKG retained; ETag recorded as rejected
}
```

---

### File: `crates/pavis/src/agent/worker/agent.rs`

**Function:** `poll_once()` (lines 151-211)

**Design decision:** Use parameter `wait_ms: u64` to communicate whether long-polling was used, allowing `start_service()` to adjust sleep behavior accordingly.

**Current logic:**
1. Build request with `if-none-match: <last_checksum>`.
2. On 200: check checksum, detect version gap, fetch intermediate versions, apply latest.
3. On 304/204: return `NoChange`.

**New signature and logic:**

```rust
pub async fn poll_once(&self, wait_ms: u64) -> anyhow::Result<PollOutcome> {
    // Use helper to get conditional ETag (prefer rejected over applied)
    let conditional_etag = self.get_conditional_etag();

    let url = format!("{}/v1/config?wait_ms={wait_ms}", self.relay_base);
    let mut request = self.client.get(url);
    if let Some(etag) = conditional_etag.as_deref() {
        request = request.header("if-none-match", format!("\"{etag}\""));
    }
    let response = request.send().await?;

    match response.status().as_u16() {
        200 => {
            let header_etag = response
                .headers()
                .get(ETAG_HEADER)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("missing {ETAG_HEADER} response header"))?;
            let header_etag = parse_etag_header(header_etag)?;

            let config_version = response
                .headers()
                .get(CONFIG_VERSION_HEADER)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_config_version_header);
            let config_size = response
                .headers()
                .get(CONFIG_SIZE_HEADER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());

            // ETag identity check against last_applied_etag
            if self.is_etag_current(&header_etag) {
                self.record_config_stats(config_version, config_size, "current etag");
                tracing::debug!(
                    etag = header_etag,
                    "config etag unchanged, skipping update"
                );
                return Ok(PollOutcome::NoChange);
            }

            // Check if this is a previously rejected ETag (relay contract violation)
            if self.is_etag_rejected(&header_etag) {
                self.record_config_stats(config_version, config_size, "rejected etag (relay violation)");
                tracing::error!(
                    etag = header_etag,
                    "relay returned 200 for previously rejected ETag; expected 304 (relay contract violation)"
                );
                return Ok(PollOutcome::NoChange);
            }

            // Attempt apply
            let bytes = response.bytes().await?;
            match self.apply_update(bytes.to_vec(), header_etag.clone(), config_version).await {
                Ok(()) => Ok(PollOutcome::Updated),
                Err(err) => {
                    // Record rejection, continue serving LKG
                    self.set_last_rejected_etag(header_etag);
                    tracing::warn!(
                        error = %err,
                        "config validation failed; continuing with LKG"
                    );
                    Ok(PollOutcome::Rejected)
                }
            }
        }
        204 | 304 => Ok(PollOutcome::NoChange),
        status => Err(anyhow::anyhow!("poll failed: status={status}")),
    }
}
```

**Key changes:**
- **Signature:** `poll_once(&self, wait_ms: u64)` - caller passes wait_ms for relay request.
- Use `self.get_conditional_etag()` helper at start (single call, no duplication).
- Use `conditional_etag` for `If-None-Match` header.
- **No sleep inside `poll_once()`** - all timing logic moved to `start_service()`.
- 304/204 => return `NoChange` immediately.
- 200 + ETag matches `last_applied_etag` => return `NoChange`.
- 200 + ETag matches `last_rejected_etag` => **relay contract violation**: log ERROR, return `NoChange`.
- 200 + new ETag => attempt apply; on success return `Updated`, on failure return `Rejected`.
- Remove lines 195-202 (version gap logic).
- Remove `fetch_and_apply_version()` call.

**Note on relay violations:** Logged at ERROR level unconditionally. No metrics recording (no new APIs required).

---

### File: `crates/pavis/src/agent/worker/agent.rs`

**Function:** `start_service()` (lines 44-70)

**Current backoff logic (lines 60-65):**
```rust
Err(err) => {
    tracing::warn!(error = %err, "config poll failed");
    let delay = self.agent.backoff.next_delay(attempt);
    attempt = attempt.saturating_add(1);
    tokio::time::sleep(delay).await;
}
```

**New backoff logic:**
```rust
let mut attempt = 0u32;
loop {
    // Compute conditional ETag and wait_ms ONCE per iteration
    let conditional_etag = self.agent.get_conditional_etag();
    let wait_ms = if conditional_etag.is_some() { 30000 } else { 0 };

    tokio::select! {
        _ = shutdown.changed() => break,
        result = self.agent.poll_once(wait_ms) => {
            match result {
                Ok(PollOutcome::Updated) => {
                    // Reset network backoff, short sleep before next poll
                    attempt = 0;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Ok(PollOutcome::NoChange) => {
                    // Reset network backoff
                    attempt = 0;
                    // Anti-busy-loop: sleep only if long-poll was NOT used (wait_ms == 0)
                    if wait_ms == 0 {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                    // Otherwise, long-poll already provided delay; no additional sleep
                }
                Ok(PollOutcome::Rejected) => {
                    // Validation failure; do NOT increment network backoff
                    // Use fixed delay to avoid tight loop
                    attempt = 0;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(err) => {
                    // Network/transport error; apply full exponential backoff
                    tracing::warn!(error = %err, "config poll failed");
                    let delay = self.agent.backoff.next_delay(attempt);
                    attempt = attempt.saturating_add(1);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
}
```

**Rationale:**
- **Once per iteration:** Compute `conditional_etag` and `wait_ms` exactly once at the top of the loop, before `tokio::select!`.
- Store `wait_ms` in a local variable; reuse it in the `NoChange` arm for the busy-loop check.
- Do NOT recompute inside match arms.
- Next iteration will compute fresh values.
- `Updated`: Reset network backoff, 1s sleep.
- `NoChange`: Reset network backoff; sleep 200ms only if `wait_ms == 0` (no long-poll).
- `Rejected`: Reset network backoff; fixed 5s sleep.
- Network errors: Full exponential backoff.

---

## 3. Apply Semantics

### File: `crates/pavis/src/agent/worker/agent.rs`

**Function:** `apply_update()` (lines 213-292)

**Changes:**

1. **Parameter rename** (line 216):
   - `expected_checksum: String` → `expected_etag: String`

2. **Variable rename** (line 219):
   - `actual_checksum` → `actual_etag`

3. **Error message** (lines 221-225):
   - "checksum mismatch: expected={}, computed={}" → "etag/sha256 mismatch: expected={}, computed={}"

4. **Success path** (lines 265, 287-290):
   - Replace `self.set_last_checksum(actual_checksum);` with:
   ```rust
   self.set_last_applied_etag(expected_etag.clone());
   self.clear_last_rejected_etag();
   ```
   - **Rationale:** Store the canonical header ETag (`expected_etag`), not the computed value (`actual_etag`). They must be equal after validation; storing the header value ensures we use the relay's canonical representation.

5. **Remove version tracking** (lines 288-290):
   - Delete:
   ```rust
   if let Some(version) = config_version {
       self.set_last_version(version);
   }
   ```

6. **Logging** (lines 252-258, 282-287):
   - Replace `checksum = expected_checksum` with `etag = expected_etag`.

**No changes to validation pipeline** (lines 227-249):
- Keep existing: checksum/verify/load/env/state validation.
- Keep existing: temp file cleanup on failure (lines 237, 245).

---

### File: `crates/pavis/src/agent/worker/agent.rs`

**Function:** `parse_etag_header()` (lines 432-445)

**Update to produce canonical representation and correctly reject weak ETags:**

```rust
fn parse_etag_header(value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();

    // Reject weak ETags BEFORE unquoting
    if trimmed.starts_with("W/") || trimmed.starts_with("w/") {
        anyhow::bail!("weak ETags not supported: {value}");
    }

    // Remove surrounding quotes if present (accept both quoted and unquoted)
    let unquoted = if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    // Validate sha256:<hex> format
    if !unquoted.starts_with("sha256:") {
        anyhow::bail!("invalid etag format (expected sha256:...): {value}");
    }
    let hex = &unquoted["sha256:".len()..];
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("invalid etag format (expected 64 hex chars): {value}");
    }

    // Return canonical form: sha256:<lowercase-hex> (no quotes, no weak prefix)
    Ok(format!("sha256:{}", hex.to_lowercase()))
}
```

**Rationale:** Canonical form ensures all ETag comparisons are reliable regardless of relay quote/case variations. Weak ETags are detected and rejected before quote stripping. Stored ETags are always `sha256:<lowercase-hex>`.

---

### File: `crates/pavis/src/agent/worker/agent.rs`

**Function:** `fetch_and_apply_version()` (lines 481-491)

**Action:** **Delete entire function.**

---

### File: `crates/pavis/src/agent/worker/agent.rs`

**Function:** `classify_validation_error()` (lines 398-415)

**Change line 411:**
- `if err.to_string().contains("checksum mismatch")` → `if err.to_string().contains("etag/sha256 mismatch")`

---

### File: `crates/pavis/src/agent/worker/agent.rs`

**Function:** `checksum_for_bytes()` (lines 417-426)

**Update to produce canonical ETag format:**

```rust
fn checksum_for_bytes(bytes: &[u8]) -> String {
    let digest = compute_checksum(bytes);
    let mut out = String::with_capacity(digest.len() * 2 + "sha256:".len());
    out.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
```

**Rationale:** Ensure computed checksums use lowercase hex to match canonical form from `parse_etag_header()`.

---

## 4. ETag Contract

### File: `crates/pavis/src/agent/worker/agent.rs`

**Add module-level documentation comment** (insert after imports, before `type UpdateCallback`):

```rust
//! # ETag Invariant
//!
//! The runtime assumes that identical `.pvs` artifact content MUST produce identical ETag values.
//!
//! **Contract:**
//! - ETag = `sha256:<hex-digest>` where digest is computed over the full artifact bytes.
//! - Canonical form: `sha256:<lowercase-hex>` (no quotes, no "W/" weak prefix).
//! - If two artifacts have the same ETag, they MUST be byte-identical.
//! - If the relay violates this (e.g., modifies artifact content without changing ETag),
//!   the runtime will skip re-downloading the artifact and continue serving stale or rejected config.
//!
//! **Violation Detection:**
//! - Relay violations will NOT surface at runtime (ETag deduplication prevents re-download).
//! - Violations are detectable only via relay-side checksumming or external audit.
//!
//! **Immutability Assumption:**
//! - Once an artifact with ETag `E` is published, the relay MUST NOT publish different content
//!   with the same ETag `E`.
//! - The runtime does NOT validate this invariant; relay integrity is the relay's responsibility.
//!
//! **Conditional Request Policy:**
//! - The runtime prefers `last_rejected_etag` over `last_applied_etag` for `If-None-Match`.
//! - This prevents repeated 200 responses when the latest artifact is rejected.
//! - The relay MUST return 304/204 when the requested ETag matches the latest, enabling long-poll.
//! - If the relay returns 200 for a previously rejected ETag, this is a **relay contract violation**:
//!   the runtime logs an error at ERROR level and returns NoChange.
```

---

## 5. Backoff & Retry Interaction

### Summary

**Existing backoff behavior:**
- `Backoff` struct (in `crates/pavis/src/agent/backoff.rs`) computes exponential delay based on `attempt` counter.
- `attempt` resets to 0 on successful poll.

**New behavior:**
- `Updated`: Reset network backoff, 1s sleep.
- `NoChange`: Reset network backoff; sleep 200ms only if `wait_ms == 0` (no long-poll).
- `Rejected`: Reset network backoff (don't pollute network counter); fixed 5s sleep.
- Network errors: Full exponential backoff.

**Tight loop prevention:**
- When rejected ETag is current: runtime sends `If-None-Match: <rejected_etag>`, relay returns 304/204, runtime returns `NoChange`, no additional sleep (long-poll delays next cycle).
- When no conditional ETag (first boot): runtime sends no `If-None-Match`, receives 204/304, sleeps 200ms in `start_service()` to prevent busy-loop.
- No repeated 200 responses; no repeated body downloads.

---

## 6. Testing Plan

### Tests to Remove/Rewrite

#### File: `crates/pavis/src/agent/worker/tests.rs`

**Remove entirely:**
- `poll_once_applies_missing_versions_in_order()` (lines 324-455)
  - **Reason:** Tests gap-replay behavior, which is explicitly removed.

**Rewrite (rename variables, remove version assertions):**

1. `apply_update_replaces_state_and_caches_checksum()` (lines 172-201)
   - **Changes:**
     - Rename `checksum` → `etag` in variable names and assertions.
     - Replace `assert_eq!(agent.last_checksum_for_tests(), Some(checksum))` → `assert_eq!(agent.last_applied_etag_for_tests(), Some(etag))`.

2. `apply_update_rejects_checksum_mismatch()` (lines 261-290)
   - **Changes:**
     - Rename `checksum` → `etag`.
     - Update error assertion: `contains("checksum mismatch")` → `contains("etag/sha256 mismatch")`.

3. `test_apply_update_success()` (lines 293-321)
   - **Changes:**
     - Rename `checksum` → `etag`.
     - Update assertion: `last_checksum_for_tests()` → `last_applied_etag_for_tests()`.

4. `poll_once_no_change_on_matching_checksum()` (lines 525-552)
   - **Changes:**
     - Rename test to `poll_once_no_change_on_matching_etag()`.
     - Rename `checksum` → `etag`.
     - Update `set_last_checksum_for_tests()` → `set_last_applied_etag_for_tests()`.
     - **Add `wait_ms` parameter:** `agent.poll_once(0).await`.

5. `test_poll_once_success()` (lines 555-604)
   - **Changes:**
     - Rename `checksum` → `etag`.
     - Update `last_checksum_for_tests()` → `last_applied_etag_for_tests()`.
     - **Add `wait_ms` parameter:** `agent.poll_once(0).await`.

**Update existing tests (add wait_ms parameter only):**
- `worker_name_is_stable()` (lines 157-169) - no changes needed
- `poll_once_returns_no_change_on_304()` (lines 204-213) - **Update:** add `wait_ms` parameter: `agent.poll_once(0).await`.
- `apply_update_removes_tmp_on_load_failure()` (lines 216-236) - no changes needed
- `poll_once_reports_non_success_status()` (lines 239-258) - **Update:** add `wait_ms` parameter: `agent.poll_once(0).await`.
- `poll_once_missing_etag_header()` (lines 497-522) - **Update:** add `wait_ms` parameter: `agent.poll_once(0).await`.

---

### New Tests to Add

#### File: `crates/pavis/src/agent/worker/tests.rs`

**Test 1: `poll_once_skips_intermediate_versions_entirely()`**

**Purpose:** Verify runtime ignores version gaps and applies only the latest snapshot.

**Setup:**
- Agent starts with `last_applied_etag = etag_v1`.
- Mock relay:
  - `/v1/config` returns status 200, etag `etag_v5`, version `5`, body `v5_bytes`.
  - `/v1/artifacts/:version` endpoint tracks fetch count.

**Execution:**
- Call `agent.poll_once(0).await`.

**Assertions:**
- `outcome` is `PollOutcome::Updated`.
- `agent.last_applied_etag_for_tests()` equals `etag_v5`.
- `on_update` callback triggered **exactly once** with service_name `"v5"`.
- Agent never fetched `/v1/artifacts/:version` (fetch count remains 0).

---

**Test 2: `poll_once_rejected_etag_triggers_304_not_200()`**

**Purpose:** Verify that after a rejection, the next poll sends `If-None-Match` with the rejected ETag and receives 304 (NoChange), not 200.

**Setup:**
- Agent starts with `last_applied_etag = etag_v1`.
- Mock relay:
  - `/v1/config` checks `If-None-Match` header.
  - If header matches `etag_bad`, return 304 immediately (no body, no waiting).
  - Otherwise, return 200 with etag `etag_bad`, body `bad_pvs_bytes` (invalid magic).

**Execution:**
1. Call `agent.poll_once(0).await` (first poll, no conditional header).
2. Call `agent.poll_once(0).await` (second poll, sends `If-None-Match: etag_bad`).
3. Call `agent.poll_once(0).await` (third poll, sends `If-None-Match: etag_bad`).

**Assertions:**
- First poll: Outcome is `Rejected`.
- Second poll: Outcome is `NoChange`.
- Third poll: Outcome is `NoChange`.
- Total 200 responses: exactly 1.
- Total 304 responses: exactly 2.
- Mock relay correctly inspects `If-None-Match` and returns 304 when it matches `etag_bad`.

---

**Test 3: `poll_once_applies_new_artifact_after_rejection()`**

**Purpose:** Verify runtime clears rejection cache and applies new artifact when ETag changes. After successful apply, subsequent polls receive 304.

**Setup:**
- Agent starts with `last_applied_etag = etag_v1`, `last_rejected_etag = etag_bad`.
- Mock relay:
  - `/v1/config` checks `If-None-Match` header.
  - If header matches `etag_bad`, return 200 with new etag `etag_v2`, body `v2_bytes` (valid).
  - If header matches `etag_v2`, return 304 immediately.

**Execution:**
1. Call `agent.poll_once(0).await` (first poll, sends `If-None-Match: etag_bad`).
2. Call `agent.poll_once(0).await` (second poll, sends `If-None-Match: etag_v2`).

**Assertions:**
- First poll: Outcome is `Updated`; `last_rejected_etag` is `None`.
- Second poll: Outcome is `NoChange`; mock returned 304.
- Mock relay correctly inspects `If-None-Match` in both polls.

---

**Test 4: `poll_once_relay_violation_200_for_rejected_etag()`**

**Purpose:** Verify that when relay incorrectly returns 200 for a previously rejected ETag (relay contract violation), the runtime returns NoChange without mutating state.

**Setup:**
- Agent starts with `last_applied_etag = etag_v1`, `last_rejected_etag = etag_bad`.
- Mock relay:
  - `/v1/config` always returns 200 with etag `etag_bad`, body `bad_pvs_bytes`.

**Execution:**
- Call `agent.poll_once(0).await`.

**Assertions:**
- Outcome is `NoChange` (not `Rejected` - we don't re-apply).
- `agent.last_applied_etag_for_tests()` still equals `etag_v1` (LKG unchanged).
- `agent.last_rejected_etag_for_tests()` still equals `etag_bad` (unchanged).
- **Do NOT assert on log text** - only assert on state and outcome behavior.

---

### Integration Test Changes

#### File: `crates/pavis/tests/config_agent_integration.rs`

**Action:** Review for gap-replay assumptions; remove or adapt tests that verify intermediate version application.

**Expected:** This file likely does not exist or does not test gap-replay. If it does, apply same removal/rewrite logic as unit tests.

---

### Relay-Side Test Responsibilities

**Out of scope for runtime changes:**
- Verifying relay publishes correct ETags.
- Verifying relay does not reuse ETags for different content.
- Verifying `/v1/artifacts/{version}` endpoint behavior (runtime no longer uses it).

**Relay test suite must cover:**
- ETag immutability enforcement.
- Checksum correctness for published artifacts.
- Long-poll behavior when version changes.
- Correct 304 responses when `If-None-Match` matches current ETag.

---

## Summary Checklist

### Code Changes
- [ ] Rename `last_checksum` → `last_applied_etag` in `ConfigAgent` struct
- [ ] Add `last_rejected_etag` field to `ConfigAgent` struct
- [ ] Remove `last_version` field from `ConfigAgent` struct
- [ ] Update `new()` and `new_for_tests()` constructors
- [ ] Remove `last_version()` / `set_last_version()` methods
- [ ] Rename `is_checksum_current()` → `is_etag_current()`
- [ ] Rename `set_last_checksum()` → `set_last_applied_etag()`
- [ ] Add `get_conditional_etag()` helper method (returns `last_rejected_etag.or(last_applied_etag)`)
- [ ] Add `is_etag_rejected()`, `set_last_rejected_etag()`, `clear_last_rejected_etag()` methods
- [ ] Add test accessors: `last_rejected_etag_for_tests()`, `set_last_rejected_etag_for_tests()`
- [ ] Rename test accessor: `last_checksum_for_tests()` → `last_applied_etag_for_tests()`
- [ ] Rename test accessor: `set_last_checksum_for_tests()` → `set_last_applied_etag_for_tests()`
- [ ] Remove test accessor: `last_version_for_tests()`
- [ ] Update `poll_once()` signature to `poll_once(&self, wait_ms: u64)`
- [ ] Rewrite `poll_once()` to use `get_conditional_etag()` helper
- [ ] Rewrite `poll_once()` to use `conditional_etag` for `If-None-Match` header
- [ ] Remove ALL sleep logic from `poll_once()` (move to `start_service()`)
- [ ] Rewrite `poll_once()` to remove gap-replay logic (lines 195-202)
- [ ] Rewrite `poll_once()` 200 response handling: check `is_etag_current()` and `is_etag_rejected()`, return `NoChange` if match
- [ ] Handle 200 with rejected ETag as relay violation: log ERROR, return `NoChange` (NO metrics recording)
- [ ] Wrap `apply_update()` in `match` to handle validation failures
- [ ] Update `PollOutcome` enum: keep exactly `Updated`, `NoChange`, `Rejected`
- [ ] Rewrite `start_service()` to compute `conditional_etag` and `wait_ms` ONCE at top of loop (before tokio::select!)
- [ ] Rewrite `start_service()` to pass `wait_ms` to `poll_once()`
- [ ] Update `start_service()` to handle `NoChange`: use SAME wait_ms variable computed at loop start for busy-loop check
- [ ] Update `start_service()` to handle `Rejected`: reset backoff counter, fixed 5s sleep
- [ ] Update `start_service()` to handle `Updated`: reset backoff, 1s sleep
- [ ] Rename parameter in `apply_update()`: `expected_checksum` → `expected_etag`
- [ ] Update error message in `apply_update()`: "etag/sha256 mismatch: expected={}, computed={}"
- [ ] Update `apply_update()` success path to store `expected_etag` (canonical header value) as `last_applied_etag`
- [ ] Update `apply_update()` success path to call `clear_last_rejected_etag()`
- [ ] Remove version tracking in `apply_update()` (lines 288-290)
- [ ] Update `parse_etag_header()` to reject weak ETags ("W/") BEFORE unquoting
- [ ] Update `parse_etag_header()` to accept both quoted and unquoted strong ETags
- [ ] Update `parse_etag_header()` to produce canonical form: `sha256:<lowercase-hex>` (no quotes, no "W/")
- [ ] Update `checksum_for_bytes()` to produce canonical form: lowercase hex
- [ ] Delete `fetch_and_apply_version()` function
- [ ] Update `classify_validation_error()` to check "etag/sha256 mismatch"
- [ ] Add ETag invariant module documentation with canonical form and relay violation handling

### Test Changes
- [ ] Remove `poll_once_applies_missing_versions_in_order()` test
- [ ] Rewrite `apply_update_replaces_state_and_caches_checksum()` (rename checksum→etag)
- [ ] Rewrite `apply_update_rejects_checksum_mismatch()` (rename checksum→etag, update error string)
- [ ] Rewrite `test_apply_update_success()` (rename checksum→etag)
- [ ] Rewrite `poll_once_no_change_on_matching_checksum()` → `poll_once_no_change_on_matching_etag()` (add wait_ms=0 parameter)
- [ ] Rewrite `test_poll_once_success()` (rename checksum→etag, add wait_ms=0 parameter)
- [ ] Update `poll_once_returns_no_change_on_304()` (add wait_ms=0 parameter)
- [ ] Update `poll_once_reports_non_success_status()` (add wait_ms=0 parameter)
- [ ] Update `poll_once_missing_etag_header()` (add wait_ms=0 parameter)
- [ ] Add `poll_once_skips_intermediate_versions_entirely()` test (validates no gap replay, wait_ms=0)
- [ ] Add `poll_once_rejected_etag_triggers_304_not_200()` test (validates If-None-Match behavior, wait_ms=0, mock responds immediately)
- [ ] Add `poll_once_applies_new_artifact_after_rejection()` test (validates rejection clearing, wait_ms=0, mock responds immediately)
- [ ] Add `poll_once_relay_violation_200_for_rejected_etag()` test (validates relay violation handling, wait_ms=0, assert behavior only)

### Validation
- [ ] Run `make ci-local` to verify all changes compile and pass tests
- [ ] Verify no regression in LKG behavior (failed updates do not affect serving traffic)
- [ ] Confirm no tight loops: rejected ETag triggers 304 responses, not repeated 200s
- [ ] Verify conditional request uses `get_conditional_etag()` helper (rejected ETag preferred over applied)
- [ ] Verify 304/204 always result in `NoChange` outcome
- [ ] Verify `Rejected` outcome only when apply_update is attempted and fails
- [ ] Verify `poll_once()` contains NO sleep logic (all timing in `start_service()`)
- [ ] Verify `apply_update()` stores canonical header ETag (`expected_etag`), not computed value
- [ ] Verify relay violation (200 for rejected ETag) logs ERROR, returns `NoChange` (no metrics recording)
- [ ] Verify all ETags stored/compared use canonical form: `sha256:<lowercase-hex>`
- [ ] Verify weak ETags are rejected before quote stripping in `parse_etag_header()`
- [ ] Verify `start_service()` computes `conditional_etag` and `wait_ms` ONCE at top of loop, reuses wait_ms in NoChange arm
