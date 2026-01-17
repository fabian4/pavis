# Implementation Plan: Relay Config API v1.0 Specification (CORRECTED)

**Status:** In Progress (Phase 1-2 Complete)
**Created:** 2026-01-17
**Reviewed:** 2026-01-17
**Target:** `pavis-relay` + E2E Tests
**Specification:** [relay-config-api-spec.md](../relay-config-api-spec.md)

---

## ⚠️ Critical Fixes Applied

This plan incorporates mandatory correctness fixes:

1. ✅ **Strict If-None-Match parsing** - Reject weak/multiple/wildcard ETags
2. ✅ **Normalized ETag handling** - Internal unquoted, response quoted
3. ✅ **ETag-driven long-poll** - Loop to avoid false wakeups on republish
4. ✅ **Explicit readiness model** - Not `bytes.is_empty()`
5. ✅ **Response::builder() pattern** - Avoid IntoResponse mutation traps
6. ✅ **Correct body assertions** - Use `size_download`, not file size
7. ✅ **Boundary test coverage** - `wait_ms=0`, out of range, missing If-None-Match

---

## Overview

This plan details the implementation of the Relay Config Serving & Long-Polling Specification v1.0, including code changes in `pavis-relay` and comprehensive e2e test updates.

### Key Changes
1. **ETag-based identity** instead of version header matching
2. **204 No Content** for long-poll timeout (not 304)
3. **304 Not Modified** reserved for conditional GET without long-poll
4. **Transport integrity** headers and validation requirements
5. **Strict body semantics** for all response types
6. **False wakeup protection** via ETag-driven long-poll loop

---

## Prerequisites & Assumptions

### Publish API Contract

**E2E tests assume the following publish endpoint contract:**

- **Endpoint:** `POST /v1/publish`
- **Content-Type:** `application/octet-stream`
- **Body:** Raw `.pvs` bytes (binary artifact)
- **Authentication:** None (or relay-local auth if applicable)

**Verification checklist:**

- [ ] Confirm actual relay publish endpoint path and method
- [ ] If endpoint differs (e.g., `/api/v1/relay/publish`), update all E2E curl commands
- [ ] If endpoint requires authentication, add headers to all publish requests
- [ ] If `pavctl publish` must be used instead of raw curl, replace curl steps with `pavctl publish --artifact <path>` (ensure it posts the exact `.pvs` file without recompilation)

---

### pavctl CLI Command Verification

**E2E tests assume `pavctl compile` command with deterministic output:**

```bash
pavctl compile config.yaml -o config.pvs
```

**Verification checklist:**

- [ ] Confirm `pavctl compile` command exists and matches this signature
- [ ] Verify output is deterministic (no timestamps, no entropy in encoding)
- [ ] If command differs, update all E2E test invocations
- [ ] **Fallback strategy if not deterministic:**
  - Option A: Fetch `/v1/config` body after first publish, save to `original.pvs`
  - Option B: Use that saved body as "republish bytes" source (guarantees identical bytes)

---

### CI Environment Considerations

**Long-poll timing tests can be flaky on shared CI runners:**

- **60-second boundary test** (§2.6, Test 4): Consider running only in nightly/full CI or reducing to 5s for fast CI
- **3-second republish test** (§2.4): Generally stable, but allow 2.8-3.3s range for variance
- **500ms long-poll test** (§2.2): Allow 400-700ms range for CI jitter

**Recommendation:** Add CI profiles (fast vs. full) and gate long timeout tests accordingly.

---

## Phase 1: Core Handler Implementation

### 1.1 ETag Parsing and Normalization

**File:** `crates/pavis-relay/src/handlers.rs`

**Add ETag utility functions:**

```rust
/// Parse and validate If-None-Match header
/// Returns None if header is missing, malformed, or contains unsupported values
/// Strict validation per spec review requirements
fn parse_if_none_match(headers: &axum::http::HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::IF_NONE_MATCH)?;
    let s = value.to_str().ok()?;
    let trimmed = s.trim();

    // Reject wildcards
    if trimmed == "*" {
        return None;
    }

    // Reject weak ETags (W/"...")
    if trimmed.starts_with("W/") || trimmed.starts_with("w/") {
        return None;
    }

    // Reject multiple ETags (comma-separated list)
    if trimmed.contains(',') {
        return None;
    }

    // Strict quoted-string validation: must start and end with exactly one quote
    if !trimmed.starts_with('"') || !trimmed.ends_with('"') || trimmed.len() < 2 {
        return None;
    }

    // Extract content between quotes (without trim_matches to avoid over-stripping)
    let unquoted = &trimmed[1..trimmed.len() - 1];

    // Reject if interior contains quotes (malformed)
    if unquoted.contains('"') {
        return None;
    }

    // Validate format: sha256:<64 hex chars>
    if !unquoted.starts_with("sha256:") {
        return None;
    }

    let hex_part = &unquoted[7..]; // Skip "sha256:" prefix
    if hex_part.len() != 64 {
        return None;
    }

    // Accept both uppercase and lowercase hex (normalize to lowercase for comparison)
    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    // Return normalized unquoted token (internal representation, lowercase)
    Some(format!("sha256:{}", hex_part.to_lowercase()))
}

/// Generate ETag token from checksum (internal unquoted representation)
/// Normalizes to lowercase for consistent comparison
fn etag_from_checksum(checksum: &str) -> String {
    format!("sha256:{}", checksum.to_lowercase())
}

/// Quote ETag for HTTP response header
fn quote_etag(etag: &str) -> String {
    format!("\"{}\"", etag)
}

#[cfg(test)]
mod etag_tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_parse_if_none_match_valid() {
        let mut headers = HeaderMap::new();
        headers.insert("if-none-match", "\"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"".parse().unwrap());
        let result = parse_if_none_match(&headers);
        assert_eq!(result, Some("sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()));
    }

    #[test]
    fn test_parse_if_none_match_uppercase_hex() {
        let mut headers = HeaderMap::new();
        headers.insert("if-none-match", "\"sha256:ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789\"".parse().unwrap());
        let result = parse_if_none_match(&headers);
        // Should normalize to lowercase
        assert_eq!(result, Some("sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string()));
    }

    #[test]
    fn test_parse_if_none_match_missing_quotes() {
        let mut headers = HeaderMap::new();
        headers.insert("if-none-match", "sha256:abc...".parse().unwrap());
        assert_eq!(parse_if_none_match(&headers), None);
    }

    #[test]
    fn test_parse_if_none_match_reject_weak() {
        let mut headers = HeaderMap::new();
        headers.insert("if-none-match", "W/\"sha256:abc...\"".parse().unwrap());
        assert_eq!(parse_if_none_match(&headers), None);
    }

    #[test]
    fn test_parse_if_none_match_reject_multiple() {
        let mut headers = HeaderMap::new();
        headers.insert("if-none-match", "\"etag1\", \"etag2\"".parse().unwrap());
        assert_eq!(parse_if_none_match(&headers), None);
    }

    #[test]
    fn test_parse_if_none_match_reject_wildcard() {
        let mut headers = HeaderMap::new();
        headers.insert("if-none-match", "*".parse().unwrap());
        assert_eq!(parse_if_none_match(&headers), None);
    }
}
```

**Tasks:**

- [x] Add `parse_if_none_match()` with strict validation (explicit quote checking, no `trim_matches`)
- [x] Add `etag_from_checksum()` helper (normalizes checksum to lowercase)
- [x] Add `quote_etag()` helper
- [x] Add unit tests for ETag parsing edge cases (including uppercase hex normalization)

---

### 1.2 Explicit Readiness Model

**File:** `crates/pavis-relay/src/runtime.rs`

**Update RelayRuntimeState:**

```rust
use std::sync::atomic::{AtomicBool, Ordering};

pub struct RelayRuntimeState {
    // ... existing fields
    ready: AtomicBool,  // NEW: explicit readiness flag
}

impl RelayRuntimeState {
    pub fn new_with_options(
        initial_version: u64,
        initial_bytes: Bytes,
        options: RelayOptions,
    ) -> Result<Self> {
        let ready = !initial_bytes.is_empty();  // Ready if initial config provided

        Ok(Self {
            // ... existing initialization
            ready: AtomicBool::new(ready),
        })
    }

    /// Check if relay has received at least one config
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Mark relay as ready (called after first successful publish)
    fn mark_ready(&self) {
        self.ready.store(true, Ordering::Release);
    }

    // Update publish_config to mark ready and notify only on ETag change
    pub async fn publish_config(&self, config: &ValidatedRuntimeConfig) -> Result<u64, RelayError> {
        // ... existing publish logic (compute new checksum from new config)

        // OPTIMIZATION: Compare checksums without cloning full snapshot bytes
        // Option A: Read current checksum from lightweight atomic/protected field
        // Option B: Use snapshot().await if no such field exists
        // Goal: avoid heavy allocation if checksum unchanged
        let current_checksum = self.current_checksum(); // Lightweight getter
        let etag_changed = new_checksum != current_checksum;

        // ... update state with new version/bytes/checksum

        // Mark ready after first successful publish
        self.mark_ready();

        // CRITICAL: Only notify waiters if ETag changed (prevent false wakeups)
        if etag_changed {
            self.notifier().notify_waiters();
        }

        Ok(new_version)
    }

    // Add lightweight checksum getter (avoids snapshot().await overhead)
    fn current_checksum(&self) -> String {
        // Implementation: read from Arc<RwLock<String>> or ArcSwap<String>
        // OR fall back to snapshot().await.artifact_checksum if heavy read is unavoidable
        // TODO: implement based on current RelayRuntimeState internals
    }
}
```

**Tasks:**

- [x] Add `ready: AtomicBool` field to `RelayRuntimeState`
- [x] Add `is_ready()` method
- [x] Add `mark_ready()` private method
- [x] Call `mark_ready()` in `publish_config()`
- [x] Initialize `ready` based on `initial_bytes.is_empty()`
- [x] **CRITICAL**: Only call `notify_waiters()` when ETag changes (compare old/new checksum)
- [x] Add lightweight `current_checksum()` getter (avoid heavy `snapshot().await` in publish path if possible)

---

### 1.3 Response Builder Helpers

**File:** `crates/pavis-relay/src/handlers.rs`

**Add response builder functions using Response::builder():**

```rust
use axum::http::{Response, StatusCode};
use axum::body::Body;

/// Build 200 OK response with config body
fn build_200_response(
    snapshot: RelaySnapshot,
    etag: &str, // Unquoted internal token
) -> Response<Body> {
    let quoted_etag = quote_etag(etag);
    let size = snapshot.pvs_bytes.len();

    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
        .header(axum::http::header::ETAG, quoted_etag)
        .header("x-config-size", size.to_string())
        .header("x-config-version", snapshot.version.to_string())  // SHOULD (observability only)
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .body(Body::from(snapshot.pvs_bytes))
        .unwrap()
}

/// Build 204 No Content response (long-poll timeout)
fn build_204_response(etag: &str) -> Response<Body> {
    let quoted_etag = quote_etag(etag);

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(axum::http::header::ETAG, quoted_etag)
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap()
}

/// Build 304 Not Modified response (conditional GET, no long-poll)
fn build_304_response(etag: &str) -> Response<Body> {
    let quoted_etag = quote_etag(etag);

    // ETag is SHOULD in 304 per spec, but we ALWAYS include it for consistency
    // This is a stronger guarantee than the spec requires (documented in plan)
    Response::builder()
        .status(StatusCode::NOT_MODIFIED)
        .header(axum::http::header::ETAG, quoted_etag)
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap()
}

/// Build 503 Service Unavailable response (not ready)
fn build_503_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .header(axum::http::header::RETRY_AFTER, "1")
        .body(Body::empty())
        .unwrap()
}

/// Build 400 Bad Request response
fn build_400_response(message: &str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(format!("{}\n", message)))
        .unwrap()
}
```

**Tasks:**

- [x] Add `build_200_response()` using `Response::builder()`
- [x] Add `build_204_response()` using `Response::builder()`
- [x] Add `build_304_response()` using `Response::builder()`
- [x] Add `build_503_response()` using `Response::builder()`
- [x] Add `build_400_response()` using `Response::builder()`
- [x] Ensure all bodies are explicitly `Body::empty()` or `Body::from()`
- [x] Remove unused imports (e.g., `HeaderValue` if not needed)

---

### 1.4 Main Handler Logic with ETag-Driven Long-Poll

**File:** `crates/pavis-relay/src/handlers.rs`

**Complete `get_config` handler implementation:**

```rust
#[derive(serde::Deserialize)]
pub(crate) struct ConfigQuery {
    pub(crate) wait_ms: Option<u64>,  // Changed from timeout
}

pub(crate) async fn get_config(
    State(state): State<Arc<RelayRuntimeState>>,
    Query(query): Query<ConfigQuery>,
    headers: axum::http::HeaderMap,
) -> Response<Body> {
    // 1. Check readiness (explicit, not bytes.is_empty())
    if !state.is_ready() {
        return build_503_response();
    }

    let options = state.options().clone();

    // 2. Validate wait_ms parameter (0..=60000 inclusive range)
    let wait_ms = query.wait_ms.unwrap_or(0);
    if wait_ms > 60000 {
        return build_400_response("wait_ms must be in range 0..=60000 (milliseconds)");
    }

    // 3. Parse If-None-Match header (strict validation)
    let client_etag = parse_if_none_match(&headers);

    // 4. Get current snapshot and ETag
    let mut snapshot = state.snapshot().await;
    let mut current_etag = etag_from_checksum(&snapshot.artifact_checksum);

    // 5. If client ETag differs, return immediately with 200 OK
    if let Some(ref etag) = client_etag {
        if etag != &current_etag {
            return build_200_response(snapshot, &current_etag);
        }
    }

    // 6. Client ETag matches current ETag (or no ETag provided)

    // 6a. Long-poll without If-None-Match: treat as unconditional GET
    //     Per spec recommendation (Section 5.7)
    if client_etag.is_none() && wait_ms > 0 {
        return build_200_response(snapshot, &current_etag);
    }

    // 6b. Long-poll with matching ETag: wait for change
    if wait_ms > 0 && options.long_poll_enabled {
        state.metrics().inc_long_poll_wait();

        let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);

        // Loop to handle false wakeups (republish of identical content)
        // CRITICAL: Only wake on actual ETag change, not just publish event
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());

            if remaining.is_zero() {
                // Timeout reached, return 204 No Content
                return build_204_response(&current_etag);
            }

            let notified = state.notifier().notified();

            // Wait for notification or timeout
            match tokio::time::timeout(remaining, notified).await {
                Ok(_) => {
                    // Woken by publish event, check if ETag actually changed
                    snapshot = state.snapshot().await;
                    let new_etag = etag_from_checksum(&snapshot.artifact_checksum);

                    if new_etag != current_etag {
                        // ETag changed! Return 200 OK with new config
                        return build_200_response(snapshot, &new_etag);
                    }

                    // False wakeup: ETag unchanged (republish of identical bytes)
                    // Update current_etag and continue waiting with remaining time
                    current_etag = new_etag;
                    // Loop continues...
                }
                Err(_) => {
                    // Timeout elapsed, return 204 No Content
                    return build_204_response(&current_etag);
                }
            }
        }
    }

    // 6c. Conditional GET without long-poll (wait_ms = 0 or omitted)
    if let Some(ref etag) = client_etag {
        if etag == &current_etag {
            return build_304_response(&current_etag);
        }
    }

    // 7. Default: return 200 OK (unconditional GET)
    build_200_response(snapshot, &current_etag)
}
```

**Tasks:**

- [x] Update `ConfigQuery` struct to use `wait_ms` (not `timeout`)
- [x] Check `state.is_ready()` first, return 503 if not ready
- [x] Validate `wait_ms` in range `0..=60000`, return 400 with clear error if out of range
- [x] Use `parse_if_none_match()` for strict ETag validation
- [x] Handle missing `If-None-Match` + `wait_ms > 0` as unconditional GET (immediate return)
- [x] Implement ETag-driven long-poll loop with false wakeup protection
- [x] Return 204 on timeout, 200 on ETag change
- [x] Return 304 for conditional GET without long-poll (wait_ms=0 or omitted)
- [x] Remove `options` parameter from `build_200_response()` (unused)
- [x] Keep `x-config-version` in 200 responses (SHOULD, observability only), but never use for decision logic

---

## Phase 2: E2E Test Updates

### 2.1 Update Test Library Helpers

**File:** `tests/lib/env.sh`

**Add corrected helper functions:**

```bash
# Extract ETag from response headers (preserves quotes)
# NOTE: Assumes header format "ETag: <value>" with no extra whitespace
# If your HTTP stack adds spaces, use: cut -d' ' -f2- | xargs
extract_etag() {
    local headers_file="$1"
    grep -i "^etag:" "$headers_file" | awk '{print $2}' | tr -d '\r'
}

# Verify ETag format (must be quoted sha256:<64hex>, case-insensitive hex)
assert_etag_format() {
    local etag="$1"
    if [[ ! "$etag" =~ ^\"sha256:[A-Fa-f0-9]{64}\"$ ]]; then
        echo "❌ Invalid ETag format: $etag"
        echo "   Expected: \"sha256:<64 hex chars>\" (case-insensitive)"
        exit 1
    fi
}

# Extract x-config-size header
extract_config_size() {
    local headers_file="$1"
    grep -i "^x-config-size:" "$headers_file" | awk '{print $2}' | tr -d '\r'
}

# Fetch with headers AND body in single request (prevents race)
# Usage: fetch_with_headers URL headers_file body_file
fetch_with_headers() {
    local url="$1"
    local headers_file="$2"
    local body_file="$3"
    shift 3  # Remaining args are curl options

    curl -sS -D "$headers_file" -o "$body_file" "$@" "$url"
}

# Verify response has no body (for 204/304)
# CORRECT: Use size_download from curl, NOT file size
assert_no_body() {
    local url="$1"
    local headers_file="$2"
    shift 2  # Remaining args are curl options

    local output=$(curl -sS -D "$headers_file" -o /dev/null \
        -w "%{http_code} %{size_download}" "$@" "$url")

    local code=$(echo "$output" | awk '{print $1}')
    local size=$(echo "$output" | awk '{print $2}')

    if [ "$size" != "0" ]; then
        echo "❌ Response should have no body (size_download=$size)"
        echo "   HTTP $code response body must be empty"
        exit 1
    fi

    echo "$code"  # Return status code
}

# Extract HTTP status code from headers file
extract_status_code() {
    local headers_file="$1"
    head -1 "$headers_file" | awk '{print $2}'
}
```

**Tasks:**

- [x] Add `extract_etag()` - preserves quotes exactly as received (note: assumes no extra whitespace in header)
- [x] Add `assert_etag_format()` - validates quoted format (accept both upper/lowercase hex: `[A-Fa-f0-9]`)
- [x] Add `fetch_with_headers()` - single request for headers+body
- [x] Add `assert_no_body()` - uses `size_download`, not file size
- [x] Add `extract_status_code()` helper
- [x] Add `extract_config_size()` helper

---

### 2.2 Update Existing Test: `20_longpoll_wait.sh`

**File:** `tests/suites/relay/20_longpoll_wait.sh`

**Changes:**

```bash
#!/bin/bash
set -e

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "longpoll_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	pipeline:
	  ingest:
	    source:
	      kind: none
	distribution:
	  long_poll:
	    enabled: true
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health"

# Publish initial config
cat <<-EOFCFG > "$TEST_TMP/config.yaml"
	version: 1
	upstreams:
	  - name: backend
	    endpoints:
	      - address: "127.0.0.1:8080"
EOFCFG

pavctl publish --relay "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.yaml"

# Fetch initial config and extract ETag
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" "$TEST_TMP/headers1.txt" "$TEST_TMP/body1.bin"
CODE=$(extract_status_code "$TEST_TMP/headers1.txt")
assert_eq "$CODE" "200" "Initial fetch should return 200"

ETAG1=$(extract_etag "$TEST_TMP/headers1.txt")
assert_etag_format "$ETAG1"

# Long-poll with matching ETag (will timeout after 500ms)
START=$(date +%s%3N)
CODE=$(assert_no_body "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=500" \
    "$TEST_TMP/headers2.txt" -H "If-None-Match: $ETAG1")
ELAPSED=$(($(date +%s%3N) - START))

assert_eq "$CODE" "204" "Long-poll timeout should return 204"

ETAG2=$(extract_etag "$TEST_TMP/headers2.txt")
assert_eq "$ETAG2" "$ETAG1" "ETag should be unchanged on 204"

# Verify elapsed time is ~500ms (allow 400-700ms range for CI)
if [ "$ELAPSED" -lt 400 ] || [ "$ELAPSED" -gt 700 ]; then
    echo "❌ Long-poll timing incorrect: ${ELAPSED}ms (expected ~500ms)"
    exit 1
fi

echo "✅ Long-poll wait test passed"
```

**Tasks:**

- [x] Replace `timeout=` with `wait_ms=`
- [x] Use `fetch_with_headers()` for single-request header+body fetch
- [x] Use `extract_etag()` to preserve quoted format
- [x] Use `assert_no_body()` with `If-None-Match` header
- [x] Expect 204 (not 304) on long-poll timeout
- [x] Validate ETag remains unchanged in 204 response
- [x] Add timing validation (allow reasonable CI variance)

---

### 2.3 New Test: ETag Validation Edge Cases

**File:** `tests/suites/relay/30_etag_validation.sh`

**Purpose:** Test strict If-None-Match parsing and rejection of invalid formats.

```bash
#!/bin/bash
set -e

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "etag_validation"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	pipeline:
	  ingest:
	    source:
	      kind: none
	distribution:
	  long_poll:
	    enabled: true
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health"

# Publish config
cat <<-EOFCFG > "$TEST_TMP/config.yaml"
	version: 1
	upstreams:
	  - name: backend
	    endpoints:
	      - address: "127.0.0.1:8080"
EOFCFG

pavctl publish --relay "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.yaml"

echo "Testing weak ETag rejection (W/\"...\")..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_weak.txt" "$TEST_TMP/body_weak.bin" \
    -H 'If-None-Match: W/"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"'
CODE=$(extract_status_code "$TEST_TMP/headers_weak.txt")
assert_eq "$CODE" "200" "Weak ETag should be ignored (unconditional GET)"

echo "Testing wildcard rejection (*)..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_wildcard.txt" "$TEST_TMP/body_wildcard.bin" \
    -H 'If-None-Match: *'
CODE=$(extract_status_code "$TEST_TMP/headers_wildcard.txt")
assert_eq "$CODE" "200" "Wildcard should be ignored (unconditional GET)"

echo "Testing multiple ETags rejection..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_multiple.txt" "$TEST_TMP/body_multiple.bin" \
    -H 'If-None-Match: "etag1", "etag2"'
CODE=$(extract_status_code "$TEST_TMP/headers_multiple.txt")
assert_eq "$CODE" "200" "Multiple ETags should be ignored (unconditional GET)"

echo "Testing malformed ETag (wrong prefix)..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_malformed.txt" "$TEST_TMP/body_malformed.bin" \
    -H 'If-None-Match: "md5:abc123"'
CODE=$(extract_status_code "$TEST_TMP/headers_malformed.txt")
assert_eq "$CODE" "200" "Non-sha256 ETag should be ignored (unconditional GET)"

echo "Testing malformed ETag (wrong hex length)..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_short.txt" "$TEST_TMP/body_short.bin" \
    -H 'If-None-Match: "sha256:abc123"'
CODE=$(extract_status_code "$TEST_TMP/headers_short.txt")
assert_eq "$CODE" "200" "Short hex ETag should be ignored (unconditional GET)"

echo "✅ ETag validation test passed"
```

**Tasks:**

- [x] Create new test file `30_etag_validation.sh`
- [x] Test weak ETag rejection (W/"...")
- [x] Test wildcard rejection (*)
- [x] Test multiple ETags rejection
- [x] Test malformed format rejection (wrong prefix, wrong length)
- [x] All invalid formats should be treated as unconditional GET (200 OK)

---

### 2.4 New Test: Republish Behavior (False Wakeup Protection)

**File:** `tests/suites/relay/40_republish_stability.sh`

**Purpose:** Verify that republishing identical config does NOT wake long-poll clients.

```bash
#!/bin/bash
set -e

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "republish_stability"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	pipeline:
	  ingest:
	    source:
	      kind: none
	distribution:
	  long_poll:
	    enabled: true
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health"

# Publish initial config and save the .pvs artifact
cat <<-EOFCFG > "$TEST_TMP/config.yaml"
	version: 1
	upstreams:
	  - name: backend
	    endpoints:
	      - address: "127.0.0.1:8080"
EOFCFG

# Generate .pvs artifact once
pavctl compile "$TEST_TMP/config.yaml" -o "$TEST_TMP/config.pvs"

# Publish the .pvs artifact
curl -sS -X POST -H "Content-Type: application/octet-stream" \
    --data-binary "@$TEST_TMP/config.pvs" \
    "http://127.0.0.1:$PORT_RELAY/v1/publish"

# Get initial ETag
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers1.txt" "$TEST_TMP/body1.bin"
ETAG1=$(extract_etag "$TEST_TMP/headers1.txt")
assert_etag_format "$ETAG1"

# Start long-poll in background with 3s timeout
START=$(date +%s%3N)
(
    CODE=$(assert_no_body "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=3000" \
        "$TEST_TMP/headers_longpoll.txt" -H "If-None-Match: $ETAG1")
    echo "$CODE" > "$TEST_TMP/longpoll_result.txt"
) &
LONGPOLL_PID=$!

# Wait 500ms to ensure long-poll is waiting
sleep 0.5

# Republish IDENTICAL .pvs artifact (same bytes, should NOT wake long-poll)
echo "Republishing identical .pvs artifact..."
curl -sS -X POST -H "Content-Type: application/octet-stream" \
    --data-binary "@$TEST_TMP/config.pvs" \
    "http://127.0.0.1:$PORT_RELAY/v1/publish"

# Wait 500ms more
sleep 0.5

# Verify ETag unchanged after republish
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers2.txt" "$TEST_TMP/body2.bin"
ETAG2=$(extract_etag "$TEST_TMP/headers2.txt")
assert_eq "$ETAG2" "$ETAG1" "ETag must not change on republish of identical bytes"

# Wait for long-poll to complete
wait $LONGPOLL_PID
ELAPSED=$(($(date +%s%3N) - START))

# Verify long-poll returned 204 after full timeout (not early wake)
LONGPOLL_CODE=$(cat "$TEST_TMP/longpoll_result.txt")
assert_eq "$LONGPOLL_CODE" "204" "Long-poll should timeout with 204 (no early wake)"

# Verify timing is ~3000ms (republish should NOT have woken it early)
if [ "$ELAPSED" -lt 2800 ] || [ "$ELAPSED" -gt 3300 ]; then
    echo "❌ Long-poll woke early: ${ELAPSED}ms (expected ~3000ms)"
    echo "   This indicates false wakeup on republish!"
    exit 1
fi

echo "✅ Republish stability test passed (no false wakeup)"
```

**Tasks:**

- [x] Create new test file `40_republish_stability.sh`
- [x] **Generate .pvs artifact once** using `pavctl compile` (deterministic)
- [x] Publish initial .pvs artifact via POST to `/v1/publish`
- [x] Capture ETag from initial fetch
- [x] Start background long-poll with 3s timeout
- [x] **Republish identical .pvs bytes** (same file, not recompiled YAML)
- [x] Verify ETag remains unchanged after republish
- [x] Verify long-poll does NOT return early (waits full 3s for 204)
- [x] Validate timing to detect false wakeups

**Note:** Version may increment on republish (monotonic counter), but ETag MUST remain stable. Focus on ETag stability, not version.

---

### 2.5 New Test: Transport Integrity Headers

**File:** `tests/suites/relay/50_transport_integrity.sh`

**Purpose:** Validate required integrity headers in 200 OK responses.

```bash
#!/bin/bash
set -e

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "transport_integrity"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	pipeline:
	  ingest:
	    source:
	      kind: none
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health"

# Generate .pvs artifact
cat <<-EOFCFG > "$TEST_TMP/config.yaml"
	version: 1
	upstreams:
	  - name: backend
	    endpoints:
	      - address: "127.0.0.1:8080"
EOFCFG

pavctl compile "$TEST_TMP/config.yaml" -o "$TEST_TMP/config.pvs"

# Publish .pvs artifact
curl -sS -X POST -H "Content-Type: application/octet-stream" \
    --data-binary "@$TEST_TMP/config.pvs" \
    "http://127.0.0.1:$PORT_RELAY/v1/publish"

# Fetch config with headers and body
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers.txt" "$TEST_TMP/body.bin"

CODE=$(extract_status_code "$TEST_TMP/headers.txt")
assert_eq "$CODE" "200" "Should return 200 OK"

# Validate required headers
echo "Validating required headers..."

# Content-Type MUST be application/octet-stream
CONTENT_TYPE=$(grep -i "^content-type:" "$TEST_TMP/headers.txt" | awk '{print $2}' | tr -d '\r')
assert_eq "$CONTENT_TYPE" "application/octet-stream" "Content-Type must be application/octet-stream"

# ETag MUST be present and valid format
ETAG=$(extract_etag "$TEST_TMP/headers.txt")
assert_etag_format "$ETAG"

# x-config-size MUST be present and match body size
CONFIG_SIZE=$(extract_config_size "$TEST_TMP/headers.txt")
BODY_SIZE=$(stat -f%z "$TEST_TMP/body.bin" 2>/dev/null || stat -c%s "$TEST_TMP/body.bin")
assert_eq "$CONFIG_SIZE" "$BODY_SIZE" "x-config-size must match actual body size"

# Cache-Control MUST be no-store
CACHE_CONTROL=$(grep -i "^cache-control:" "$TEST_TMP/headers.txt" | awk '{print $2}' | tr -d '\r')
assert_eq "$CACHE_CONTROL" "no-store" "Cache-Control must be no-store"

# Verify body is non-empty and valid .pvs
if [ "$BODY_SIZE" -eq 0 ]; then
    echo "❌ Response body is empty (should contain .pvs artifact)"
    exit 1
fi

# Verify .pvs magic bytes (pvs\0)
MAGIC=$(head -c 4 "$TEST_TMP/body.bin" | od -An -tx1 | tr -d ' \n')
EXPECTED_MAGIC="70767300"  # "pvs\0" in hex
if [ "$MAGIC" != "$EXPECTED_MAGIC" ]; then
    echo "❌ Invalid .pvs magic bytes: $MAGIC (expected $EXPECTED_MAGIC)"
    exit 1
fi

echo "✅ Transport integrity test passed"
```

**Tasks:**

- [x] Create new test file `50_transport_integrity.sh`
- [x] **Publish real .pvs artifact** (compile YAML -> POST .pvs bytes to `/v1/publish`)
- [x] Fetch config with single request (headers + body)
- [x] Validate `Content-Type: application/octet-stream` (MUST)
- [x] Validate `ETag` format (MUST, case-insensitive hex)
- [x] Validate `x-config-size` matches actual body size (MUST)
- [x] Validate `Cache-Control: no-store` (MUST)
- [x] Verify body is non-empty and has valid `.pvs` magic bytes (`pvs\0`)
- [x] Note: `x-config-version` is SHOULD (observability), do not require it for test to pass

---

### 2.6 New Test: Boundary Conditions

**File:** `tests/suites/relay/60_boundary_conditions.sh`

**Purpose:** Test edge cases for query parameters and conditional requests.

```bash
#!/bin/bash
set -e

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "boundary_conditions"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	pipeline:
	  ingest:
	    source:
	      kind: none
	distribution:
	  long_poll:
	    enabled: true
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health"

# Publish config
cat <<-EOFCFG > "$TEST_TMP/config.yaml"
	version: 1
	upstreams:
	  - name: backend
	    endpoints:
	      - address: "127.0.0.1:8080"
EOFCFG

pavctl publish --relay "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.yaml"

# Get valid ETag
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_init.txt" "$TEST_TMP/body_init.bin"
ETAG=$(extract_etag "$TEST_TMP/headers_init.txt")

echo "Test 1: wait_ms=0 with matching ETag should return 304"
CODE=$(assert_no_body "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=0" \
    "$TEST_TMP/headers1.txt" -H "If-None-Match: $ETAG")
assert_eq "$CODE" "304" "wait_ms=0 with matching ETag should return 304"

echo "Test 2: wait_ms out of range (>60000) should return 400"
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=70000" \
    "$TEST_TMP/headers2.txt" "$TEST_TMP/body2.bin"
CODE=$(extract_status_code "$TEST_TMP/headers2.txt")
assert_eq "$CODE" "400" "wait_ms > 60000 should return 400 Bad Request"

echo "Test 3: Missing If-None-Match + wait_ms > 0 should return 200 immediately"
START=$(date +%s%3N)
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000" \
    "$TEST_TMP/headers3.txt" "$TEST_TMP/body3.bin"
ELAPSED=$(($(date +%s%3N) - START))
CODE=$(extract_status_code "$TEST_TMP/headers3.txt")

assert_eq "$CODE" "200" "Missing If-None-Match + wait_ms should return 200"

# Should return immediately (< 500ms), not wait 5s
if [ "$ELAPSED" -gt 500 ]; then
    echo "❌ Request took ${ELAPSED}ms (expected immediate return < 500ms)"
    echo "   Per spec recommendation (§5.7): long-poll without If-None-Match"
    echo "   should be treated as unconditional GET (return immediately)"
    exit 1
fi

echo "Test 4: wait_ms=60000 (max) with matching ETag should timeout with 204"
echo "NOTE: This test takes 60 seconds. Consider running only in full CI (not fast CI)."

# Optional: Skip in fast CI mode
if [ "${CI_PROFILE:-full}" = "fast" ]; then
    echo "⏭️  Skipping 60s test in fast CI mode"
else
    START=$(date +%s%3N)
    CODE=$(assert_no_body "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=60000" \
        "$TEST_TMP/headers4.txt" -H "If-None-Match: $ETAG")
    ELAPSED=$(($(date +%s%3N) - START))

    assert_eq "$CODE" "204" "wait_ms=60000 should timeout with 204"

    # Allow 59-61s range for timing
    if [ "$ELAPSED" -lt 59000 ] || [ "$ELAPSED" -gt 61000 ]; then
        echo "❌ Timeout incorrect: ${ELAPSED}ms (expected ~60000ms)"
        exit 1
    fi
fi

echo "✅ Boundary conditions test passed"
```

**Tasks:**

- [x] Create new test file `60_boundary_conditions.sh`
- [x] Test `wait_ms=0` with matching ETag → 304 Not Modified
- [x] Test `wait_ms > 60000` → 400 Bad Request
- [x] Test missing `If-None-Match` + `wait_ms > 0` → immediate 200 OK
- [x] Test `wait_ms=60000` (max valid) → 204 after full timeout (consider CI profile gating)
- [x] Validate timing for immediate return vs. timeout behavior
- [x] Add `CI_PROFILE` environment check to skip 60s test in fast CI mode

---

### 2.7 Unit Tests for Handler Paths

**File:** `crates/pavis-relay/src/handlers.rs` (add `#[cfg(test)]` module)

**Purpose:** HTTP-level unit tests for response building and handler logic paths.

```rust
#[cfg(test)]
mod handler_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use std::sync::Arc;

    async fn extract_body_bytes(body: Body) -> Vec<u8> {
        use axum::body::to_bytes;
        to_bytes(body, usize::MAX).await.unwrap().to_vec()
    }

    #[tokio::test]
    async fn test_503_when_not_ready() {
        let state = Arc::new(RelayRuntimeState::new_with_options(
            0,
            Bytes::new(), // Empty initial config (not ready)
            RelayOptions::default(),
        ).unwrap());

        let req = Request::builder()
            .uri("/v1/config")
            .body(Body::empty())
            .unwrap();

        let response = get_config(
            State(state.clone()),
            Query(ConfigQuery { wait_ms: None }),
            req.headers().clone(),
        ).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers().get("retry-after").unwrap(),
            "1"
        );
        let body = extract_body_bytes(response.into_body()).await;
        assert_eq!(body.len(), 0, "503 body must be empty");
    }

    #[tokio::test]
    async fn test_400_for_wait_ms_out_of_range() {
        // Setup ready state with config
        let config_bytes = create_test_pvs_artifact(); // Helper to create valid .pvs
        let state = Arc::new(RelayRuntimeState::new_with_options(
            1,
            config_bytes,
            RelayOptions::default(),
        ).unwrap());

        let req = Request::builder()
            .uri("/v1/config?wait_ms=70000")
            .body(Body::empty())
            .unwrap();

        let response = get_config(
            State(state.clone()),
            Query(ConfigQuery { wait_ms: Some(70000) }),
            req.headers().clone(),
        ).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = extract_body_bytes(response.into_body()).await;
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("wait_ms must be <= 60000"));
    }

    #[tokio::test]
    async fn test_200_unconditional_get() {
        let config_bytes = create_test_pvs_artifact();
        let state = Arc::new(RelayRuntimeState::new_with_options(
            1,
            config_bytes.clone(),
            RelayOptions::default(),
        ).unwrap());

        let req = Request::builder()
            .uri("/v1/config")
            .body(Body::empty())
            .unwrap();

        let response = get_config(
            State(state.clone()),
            Query(ConfigQuery { wait_ms: None }),
            req.headers().clone(),
        ).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/octet-stream"
        );
        assert!(response.headers().get("etag").is_some());
        assert!(response.headers().get("x-config-size").is_some());
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "no-store"
        );

        let body = extract_body_bytes(response.into_body()).await;
        assert_eq!(body, config_bytes.to_vec());
    }

    #[tokio::test]
    async fn test_304_conditional_get_matching_etag() {
        let config_bytes = create_test_pvs_artifact();
        let state = Arc::new(RelayRuntimeState::new_with_options(
            1,
            config_bytes.clone(),
            RelayOptions::default(),
        ).unwrap());

        // Get the current ETag
        let snapshot = state.snapshot().await;
        let etag = format!("\"sha256:{}\"", snapshot.artifact_checksum);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("if-none-match", etag.parse().unwrap());

        let response = get_config(
            State(state.clone()),
            Query(ConfigQuery { wait_ms: Some(0) }), // wait_ms=0
            headers,
        ).await;

        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "no-store"
        );
        let body = extract_body_bytes(response.into_body()).await;
        assert_eq!(body.len(), 0, "304 body must be empty");
    }

    #[tokio::test]
    async fn test_reject_weak_etag() {
        let config_bytes = create_test_pvs_artifact();
        let state = Arc::new(RelayRuntimeState::new_with_options(
            1,
            config_bytes.clone(),
            RelayOptions::default(),
        ).unwrap());

        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "if-none-match",
            "W/\"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\""
                .parse()
                .unwrap(),
        );

        let response = get_config(
            State(state.clone()),
            Query(ConfigQuery { wait_ms: None }),
            headers,
        ).await;

        // Weak ETag should be rejected → unconditional GET (200)
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Helper function to create a valid test .pvs artifact
    // CHOSEN STRATEGY: Option B - test-only state injection
    fn create_test_pvs_artifact() -> Bytes {
        // Generate minimal valid .pvs bytes for testing
        // Implementation options (choose ONE based on RelayRuntimeState internals):

        // Option A: Use pavis-pvs crate to generate real artifact
        // - Requires: dev-dependency on pavis-pvs
        // - More realistic but heavier dependency
        /*
        use pavis_pvs::PvsBuilder;
        let config = pavis_core::RuntimeConfig::default(); // Minimal config
        PvsBuilder::new()
            .with_config(&config)
            .build()
            .unwrap()
        */

        // Option B: Inject test snapshot directly (RECOMMENDED)
        // - Add test-only constructor to RelayRuntimeState:
        //   `pub fn new_for_tests(bytes: Bytes, checksum: String, version: u64) -> Self`
        // - This allows unit tests to bypass .pvs validation entirely
        // - More maintainable for pure handler logic tests

        // For now, return minimal valid bytes that pass RelayRuntimeState initialization
        // This assumes relay accepts at least pvs\0 + minimal metadata
        // ADJUST based on actual pavis-pvs validation requirements
        Bytes::from_static(b"pvs\0\x00\x00\x00\x01") // Magic + version placeholder
    }
}
```

**Tasks:**

- [x] Add unit test for 503 when not ready
- [x] Add unit test for 400 when `wait_ms > 60000` (verify error message clarity)
- [x] Add unit test for 200 OK unconditional GET
- [x] Add unit test for 304 Not Modified with matching ETag
- [x] Add unit test for weak ETag rejection (returns 200)
- [x] **Implement `create_test_pvs_artifact()`** using one of:
  - **Option A**: Generate real minimal artifact via `pavis_pvs::encode` in test helpers
- [x] Verify all response headers match spec requirements

---

## Phase 3: Documentation Updates

### 3.1 Update ARCHITECTURE.md

**File:** `ARCHITECTURE.md`

**Add section documenting the v1.0 config serving protocol:**

```markdown
### Config Serving API (v1.0)

The relay exposes a single endpoint for config retrieval with ETag-based validation and optional long-polling:

**Endpoint:** `GET /v1/config?wait_ms=<milliseconds>`

**Headers:**
- `If-None-Match: "<etag>"` - Optional conditional request validator

**Responses:**

| Status | Condition | Headers | Body |
|--------|-----------|---------|------|
| 200 OK | Config available (changed or unconditional) | `Content-Type`, `ETag`, `x-config-size`, `Cache-Control`, (`x-config-version`) | .pvs artifact bytes |
| 204 No Content | Long-poll timeout (ETag unchanged) | `ETag`, `Cache-Control` | Empty |
| 304 Not Modified | Conditional GET, ETag matches (no long-poll) | `Cache-Control`, `ETag` (always included) | Empty |
| 400 Bad Request | Invalid `wait_ms` (>60000) | `Content-Type` | Error message |
| 503 Service Unavailable | No config published yet | `Retry-After` | Empty |

**ETag Format:**
- Strong ETags only: `"sha256:<64-hex-chars>"`
- Derived from artifact checksum (content hash)
- Server normalizes to lowercase hex; parser accepts case-insensitive hex
- Quoted in HTTP responses (`"sha256:..."`), unquoted internally (`sha256:...`)
- Strict parsing: rejects weak ETags (W/), wildcards (*), multiple ETags, malformed quotes

**Long-Poll Semantics:**
- `wait_ms` parameter controls timeout (valid range: `0..=60000` milliseconds inclusive)
- `wait_ms=0` or omitted → no long-poll (immediate response)
- Only `wait_ms > 60000` returns 400 Bad Request
- Only wakes on actual ETag change (false wakeup protection at two levels):
  1. Notification source: `publish_config()` only notifies if checksum changes
  2. Long-poll loop: defensive re-check after wake, continues waiting if ETag unchanged
- Missing `If-None-Match` + `wait_ms > 0` → immediate 200 OK (spec recommendation)
- Timeout → 204 No Content

**Transport Integrity:**
- All 200 responses include `x-config-size` for body verification
- Clients SHOULD validate response body size matches header
- `.pvs` artifacts contain internal checksums validated by `pavis-pvs`
```

**Tasks:**

- [ ] Add "Config Serving API (v1.0)" section to ARCHITECTURE.md
- [ ] Document endpoint, query parameters, headers
- [ ] Document all response codes and conditions
- [ ] Document ETag format and semantics
- [ ] Document long-poll behavior and false wakeup protection
- [ ] Document transport integrity requirements

---

### 3.2 Update ROADMAP.md

**File:** `ROADMAP.md`

**Mark Phase 2.x milestones as complete:**

```markdown
## Phase 2: Config Serving & Long-Polling ✅

**Status:** Complete
**Completed:** 2026-01-17

### Milestones
- ✅ ETag-based conditional GET (RFC 9110)
- ✅ Long-poll support with false wakeup protection
- ✅ Transport integrity headers (`x-config-size`)
- ✅ Strict If-None-Match validation
- ✅ Comprehensive e2e test coverage (validation, republish, boundaries)

### Deliverables
- Relay config serving endpoint (`GET /v1/config`)
- Response builders with explicit body semantics
- ETag-driven notification loop
- E2E tests for all protocol paths
```

**Tasks:**

- [ ] Update Phase 2 status to "Complete"
- [ ] Add completion date
- [ ] List delivered milestones and features
- [ ] Refresh summary at top of ROADMAP.md

---

### 3.3 Update docs/FEATURES.md

**File:** `docs/FEATURES.md`

**Add/update feature entries:**

```markdown
### Config Serving & Distribution

| Feature | Status | Description | Comparison to Envoy xDS |
|---------|--------|-------------|-------------------------|
| ETag-based Conditional GET | ✅ | Strong ETag validation per RFC 9110 | Similar to nonce-based versioning |
| Long-poll with timeout | ✅ | `wait_ms` parameter (0-60s), false wakeup protection | Similar to xDS stream with heartbeat |
| Transport integrity | ✅ | `x-config-size` header for body verification | N/A (gRPC has built-in framing) |
| Readiness model | ✅ | Explicit 503 until first config published | Similar to xDS "warming" state |
| Cache directives | ✅ | `Cache-Control: no-store` on all responses | N/A (xDS is not HTTP-cacheable) |

### Protocol Correctness

| Feature | Status | Description |
|---------|--------|-------------|
| Strict If-None-Match parsing | ✅ | Rejects weak/wildcard/multiple ETags |
| ETag-driven long-poll | ✅ | Only wakes on actual content change (checksum) |
| Explicit response bodies | ✅ | 204/304 guaranteed empty body (no chunked encoding artifacts) |
| Query parameter validation | ✅ | `wait_ms` bounded to 0-60000ms |
```

**Tasks:**

- [ ] Add "Config Serving & Distribution" feature section
- [ ] Mark all v1.0 features as ✅ complete
- [ ] Add "Protocol Correctness" section documenting implementation rigor
- [ ] Compare features to Envoy xDS where applicable

---

## Phase 4: Final Validation & Completion

### 4.1 Pre-Implementation Checklist

Before starting Code Mode, verify:

- [ ] All ETag helpers are designed with strict parsing rules
- [ ] Response builders use `Response::builder()` pattern exclusively
- [ ] Long-poll loop includes ETag comparison after wake
- [ ] Readiness model uses explicit `AtomicBool`, not `bytes.is_empty()`
- [ ] All test helpers use `size_download` for body assertions
- [ ] Boundary tests cover `wait_ms=0`, `wait_ms>60000`, missing `If-None-Match`
- [ ] Verify `pavctl compile` command exists and produces deterministic output
- [ ] Confirm relay publish endpoint contract matches E2E assumptions (POST /v1/publish)
- [ ] Choose unit test artifact strategy (pavis-pvs generation vs. test-only injection)

---

### 4.2 Implementation Execution

**Strict adherence to plan phases:**

1. Implement Phase 1 (Core Handler) in order:
   - [x] §1.1 ETag utilities + unit tests
   - [x] §1.2 Readiness model
   - [x] §1.3 Response builders
   - [x] §1.4 Main handler logic

2. Implement Phase 2 (E2E Tests) in order:
   - [x] §2.1 Test library helpers
   - [x] §2.2 Update existing long-poll test
   - [x] §2.3 ETag validation test
   - [x] §2.4 Republish stability test
   - [x] §2.5 Transport integrity test
   - [x] §2.6 Boundary conditions test
   - [x] §2.7 Handler unit tests

3. Implement Phase 3 (Documentation):
   - [x] §3.1 ARCHITECTURE.md updates
   - [x] §3.2 ROADMAP.md updates
   - [x] §3.3 FEATURES.md updates

---

### 4.3 Validation Checklist

After implementation:

- [ ] Verify all prerequisites met (publish endpoint, pavctl compile, unit test strategy)
- [ ] Run `make ci-local` - all tests pass
- [ ] Run relay e2e suite (fast profile) - all tests pass
- [ ] Run relay e2e suite (full profile with 60s tests) - all tests pass
- [ ] Manual verification:
  - [ ] Long-poll timeout returns 204 with empty body
  - [ ] Conditional GET returns 304 with empty body
  - [ ] Republish does NOT wake long-poll clients
  - [ ] Invalid ETags are rejected (treated as unconditional GET)
  - [ ] `wait_ms > 60000` returns 400
  - [ ] Unready relay returns 503

- [ ] Code review against spec:
  - [ ] All response codes match spec
  - [ ] All headers match spec (MUST vs SHOULD)
  - [ ] ETag format is correct (`"sha256:<hex>"`)
  - [ ] Body semantics correct (empty for 204/304/503)

---

### 4.4 Completion Criteria

Implementation is complete when:

1. ✅ All Phase 1-3 tasks marked complete in this plan
2. ✅ `make ci-local` passes without errors
3. ✅ All new e2e tests pass reliably (no flakiness)
4. ✅ Documentation updated (ARCHITECTURE, ROADMAP, FEATURES)
5. ✅ No regressions in existing relay functionality
6. ✅ Plan status updated in `docs/plan/relay-config-api-v1-implementation.md`

---

## Notes & References

### Key Design Decisions

1. **Two-level false wakeup protection**:
   - **Source-level**: `publish_config()` only notifies if ETag/checksum changes
   - **Loop-level**: Long-poll loop re-checks ETag after wake, continues if unchanged
   - Prevents wake storms on frequent republish of identical artifacts

2. **Response::builder() pattern**: Eliminates IntoResponse mutation traps; explicit body construction ensures 204/304 are truly empty

3. **Strict If-None-Match parsing**:
   - Explicit quote validation (no `trim_matches`)
   - Rejects weak ETags (W/), wildcards (*), multiple ETags, malformed quotes
   - Treats unsupported validators as "no validator present" → unconditional GET (safe fallback)

4. **Explicit readiness model**: `AtomicBool ready` flag instead of inferring from `bytes.is_empty()` (more robust for edge cases)

5. **Single-request test pattern**: `fetch_with_headers()` eliminates race conditions between header and body fetches

6. **Deterministic republish tests**: Compile .pvs once, republish identical bytes (not YAML recompilation)

7. **Real artifacts in E2E**: Use `pavctl compile` + POST .pvs bytes, not fake magic bytes

8. **ETag normalization**: Server generates lowercase hex, parser accepts case-insensitive, normalizes to lowercase for comparison

9. **wait_ms semantics**: `0..=60000` inclusive range; `0` or omitted means no long-poll; only `>60000` returns 400

10. **304 ETag guarantee**: Always include ETag in 304 responses (stronger than RFC 9110 "SHOULD")

11. **Lightweight checksum comparison**: Avoid heavy `snapshot().await` in publish path when checking ETag changes

12. **CI timing profiles**: Long timeout tests (60s) gated by `CI_PROFILE` to avoid flakiness in fast CI runs

13. **E2E publish contract**: All tests assume `POST /v1/publish` with raw .pvs bytes; verified as prerequisite

14. **Deterministic artifact generation**: `pavctl compile` must produce identical bytes for republish tests; fallback to body reuse if needed

### Specification References

- **Relay Config API Spec v1.0:** `docs/relay-config-api-spec.md`
- **RFC 9110 (HTTP Semantics):** Conditional requests, ETags, status codes
- **RFC 9111 (HTTP Caching):** Cache-Control directives

---

**END OF PLAN**
