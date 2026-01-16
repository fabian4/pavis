# Execution Plan: Relay Versioning E2E Test Suite

**Status Legend:** [ ] pending, [x] complete, [-] not applicable

**Goal:** Implement comprehensive E2E test coverage for "Relay Versioning & History Storage (Final Spec)" using shell-based tests with deterministic crash recovery simulation.

---

## 1. Overview

### 1.1. Scope
- **6 E2E test scripts** covering all critical spec behaviors
- **Shared test harness** for relay lifecycle and assertions
- **Failpoint mechanism** for deterministic crash simulation (feature-gated)
- **CI integration** for linux/amd64 and linux/arm64

### 1.2. Constraints
- No new crates
- Shell-based tests under `tests/suites/integrated/`
- Deterministic, isolated, self-cleaning
- Cross-platform (linux/amd64, linux/arm64)
- Feature-gated failpoints (off by default)

### 1.3. Test Files
1. `50_relay_publish_and_versioning.sh` - Version generation, idempotency
2. `51_relay_config_longpoll_and_checksum.sh` - Config serving, headers, long-poll
3. `52_agent_checksum_dedup.sh` - Client deduplication behavior
4. `53_relay_restart_and_state_cache.sh` - State cache repair from LKG
5. `54_relay_crash_recovery_matrix.sh` - Crash recovery invariants (uses failpoints)
6. `55_history_integrity_and_orphans.sh` - History layout, orphan handling

---

## 2. Shared Test Harness Design

### 2.1. Helper Library: `_lib_relay.sh`

**Location:** `tests/suites/integrated/_lib_relay.sh`

**Purpose:** Common functions for relay lifecycle, HTTP operations, assertions

**Functions to Implement:**

#### Lifecycle Management
```bash
# mk_tmpdir <name>
# - Creates isolated temp directory for test data
# - Returns absolute path
# - Registers cleanup trap
mk_tmpdir() { ... }

# start_relay <data_dir> <port>
# - Compiles relay with appropriate features
# - Starts relay in background: pavis-relay --data-dir=$data_dir --bind=0.0.0.0:$port
# - Waits for health check
# - Returns relay PID
# - Registers cleanup trap to kill on exit
start_relay() { ... }

# start_relay_with_failpoint <data_dir> <port> <failpoint_name>
# - Same as start_relay but sets PAVIS_RELAY_FAILPOINT env var
# - Used only in test 54
start_relay_with_failpoint() { ... }

# stop_relay <pid>
# - Sends SIGTERM to relay process
# - Waits up to 5s for graceful shutdown
# - Returns exit code
stop_relay() { ... }

# wait_http_ready <url> [timeout_s]
# - Polls GET $url/health until 200 OK
# - Default timeout: 10s
# - Returns 0 on success, 1 on timeout
wait_http_ready() { ... }
```

#### HTTP Operations
```bash
# http_publish <port> <pvs_file>
# - POST $pvs_file to http://localhost:$port/v1/publish
# - Returns JSON response body to stdout
# - Fails if non-200 status
http_publish() { ... }

# http_get_config <port> <timeout_s> <out_body_file> <out_hdr_file>
# - GET http://localhost:$port/v1/config?timeout=$timeout_s
# - Writes response body to $out_body_file
# - Writes headers (one per line) to $out_hdr_file
# - Returns HTTP status code
http_get_config() { ... }

# http_get_config_async <port> <timeout_s> <out_body_file> <out_hdr_file> <out_pid_file>
# - Same as http_get_config but runs in background
# - Writes background PID to $out_pid_file for later wait
http_get_config_async() { ... }

# http_status <port>
# - GET http://localhost:$port/v1/status
# - Returns JSON response body to stdout
http_status() { ... }
```

#### Helpers
```bash
# sha256_file <path>
# - Computes SHA256 of file
# - Returns "sha256:<64 hex chars>"
sha256_file() { ... }

# extract_header <hdr_file> <header_name>
# - Parses header file and returns value
# - Case-insensitive header name matching
extract_header() { ... }

# json_field <json_string> <field_path>
# - Extracts field from JSON using jq
# - Example: json_field "$resp" ".version"
json_field() { ... }

# find_free_port
# - Returns an available TCP port
# - Used to avoid port conflicts between tests
find_free_port() { ... }
```

#### Assertions
```bash
# assert_eq <actual> <expected> [msg]
# - Fails test if values don't match
# - Prints diagnostic message
assert_eq() { ... }

# assert_ne <actual> <expected> [msg]
# - Fails test if values match
assert_ne() { ... }

# assert_contains <haystack> <needle> [msg]
# - Fails if needle not found in haystack
assert_contains() { ... }

# assert_file_exists <path> [msg]
# - Fails if file doesn't exist
assert_file_exists() { ... }

# assert_file_not_exists <path> [msg]
# - Fails if file exists
assert_file_not_exists() { ... }

# assert_http_status <actual> <expected> [msg]
# - Specialized for HTTP status code comparison
assert_http_status() { ... }
```

---

## 3. Failpoint Mechanism Design

### 3.1. Feature Flag

**Location:** `crates/pavis-relay/Cargo.toml`

```toml
[features]
default = []
relay-failpoints = []
```

### 3.2. Implementation

**Location:** `crates/pavis-relay/src/failpoints.rs` (new file)

```rust
#[cfg(feature = "relay-failpoints")]
pub fn check_failpoint(name: &str) {
    if let Ok(fp) = std::env::var("PAVIS_RELAY_FAILPOINT") {
        if fp == name {
            eprintln!("FAILPOINT TRIGGERED: {}", name);
            std::process::exit(42); // Distinct exit code for failpoint
        }
    }
}

#[cfg(not(feature = "relay-failpoints"))]
pub fn check_failpoint(_name: &str) {
    // No-op when feature disabled
}
```

### 3.3. Integration Points

**Location:** `crates/pavis-relay/src/handlers.rs` (publish handler)

Insert failpoint checks at critical boundaries:

```rust
// After validation
pavis_relay::failpoints::check_failpoint("after_validation");

// After history write
pavis_relay::failpoints::check_failpoint("after_history_write");

// After LKG artifact write
pavis_relay::failpoints::check_failpoint("after_lkg_artifact_write");

// After LKG metadata write
pavis_relay::failpoints::check_failpoint("after_lkg_meta_write");

// After state.json write
pavis_relay::failpoints::check_failpoint("after_state_write");
```

### 3.4. Compilation Strategy

**Test 54 only:**
```bash
# Compile relay with failpoints enabled
cargo build --release --features relay-failpoints -p pavis-relay

# Run test with this binary
PAVIS_RELAY_BIN=./target/release/pavis-relay \
  PAVIS_RELAY_FAILPOINT=after_history_write \
  ./tests/suites/integrated/54_relay_crash_recovery_matrix.sh
```

**Other tests:** Use standard relay binary (no failpoints)

---

## 4. Test Specifications

### 4.1. Test 50: Publish and Versioning

**File:** `tests/suites/integrated/50_relay_publish_and_versioning.sh`

**Coverage:**
- Monotonic version generation (v+1)
- Version 0 sentinel (bootstrap state)
- Idempotency (same artifact → new version, SAME checksum)
- LKG promotion
- History storage creation
- Status endpoint fields

**Implementation:**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/_lib_relay.sh"

main() {
  echo "[TEST 50] Relay publish and versioning"

  # Setup
  local data_dir=$(mk_tmpdir "test50")
  local port=$(find_free_port)

  # Start relay with empty data dir
  local relay_pid=$(start_relay "$data_dir" "$port")

  # Test 1: Bootstrap state (version 0)
  local status=$(http_status "$port")
  local version=$(json_field "$status" ".current_version")
  assert_eq "$version" "0" "Bootstrap version should be 0"

  # Create test artifacts
  local artifact_a="$data_dir/test_a.pvs"
  pavctl gen examples/basic.yaml "$artifact_a"
  local checksum_a=$(sha256_file "$artifact_a")

  # Test 2: First publish (version 1)
  local resp=$(http_publish "$port" "$artifact_a")
  local v1=$(json_field "$resp" ".version")
  local cs1=$(json_field "$resp" ".checksum")
  assert_eq "$v1" "1" "First publish should be version 1"
  assert_eq "$cs1" "$checksum_a" "Checksum should match artifact"

  # Verify history and LKG created
  assert_file_exists "$data_dir/history/0000000001.pvs"
  assert_file_exists "$data_dir/history/0000000001.meta.json"
  assert_file_exists "$data_dir/lkg/config.pvs"
  assert_file_exists "$data_dir/lkg/meta.json"

  # Test 3: Idempotent publish (same artifact, version 2, SAME checksum)
  local resp2=$(http_publish "$port" "$artifact_a")
  local v2=$(json_field "$resp2" ".version")
  local cs2=$(json_field "$resp2" ".checksum")
  assert_eq "$v2" "2" "Second publish should be version 2"
  assert_eq "$cs2" "$checksum_a" "Checksum should be SAME (deterministic)"

  # Verify LKG updated to version 2
  local lkg_meta=$(cat "$data_dir/lkg/meta.json")
  local lkg_ver=$(json_field "$lkg_meta" ".version")
  assert_eq "$lkg_ver" "2" "LKG version should be 2"

  # Test 4: Different artifact (version 3, different checksum)
  local artifact_b="$data_dir/test_b.pvs"
  # Modify config slightly
  sed 's/basic/modified/g' examples/basic.yaml > /tmp/modified.yaml
  pavctl gen /tmp/modified.yaml "$artifact_b"
  local checksum_b=$(sha256_file "$artifact_b")

  local resp3=$(http_publish "$port" "$artifact_b")
  local v3=$(json_field "$resp3" ".version")
  local cs3=$(json_field "$resp3" ".checksum")
  assert_eq "$v3" "3" "Third publish should be version 3"
  assert_ne "$cs3" "$checksum_a" "Different artifact should have different checksum"
  assert_eq "$cs3" "$checksum_b" "Checksum should match artifact B"

  # Test 5: Status endpoint accuracy
  local final_status=$(http_status "$port")
  local final_ver=$(json_field "$final_status" ".current_version")
  local hist_count=$(json_field "$final_status" ".history_count")
  assert_eq "$final_ver" "3" "Final version should be 3"
  assert_eq "$hist_count" "3" "History count should be 3"

  # Cleanup
  stop_relay "$relay_pid"

  echo "[TEST 50] PASSED"
}

main "$@"
```

**Tasks:**
- [ ] Implement test script
- [ ] Create sample PVS artifacts (or generate on-the-fly)
- [ ] Verify history file naming format (10-digit padding)
- [ ] Test passes with empty data dir
- [ ] Test passes on re-run (cleanup works)

---

### 4.2. Test 51: Config Long-Poll and Checksum Headers

**File:** `tests/suites/integrated/51_relay_config_longpoll_and_checksum.sh`

**Coverage:**
- GET /v1/config returns LKG with checksum headers
- X-Config-Checksum matches sha256(body)
- X-Config-Size matches body size
- X-Config-Version present (observability)
- Long-poll timeout behavior (idempotent)
- Long-poll wake on publish

**Implementation:**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/_lib_relay.sh"

main() {
  echo "[TEST 51] Config long-poll and checksum headers"

  local data_dir=$(mk_tmpdir "test51")
  local port=$(find_free_port)
  local relay_pid=$(start_relay "$data_dir" "$port")

  # Publish artifact A
  local artifact_a="$data_dir/test_a.pvs"
  pavctl gen examples/basic.yaml "$artifact_a"
  http_publish "$port" "$artifact_a"

  # Test 1: GET /v1/config returns correct headers
  local body_file="$data_dir/body1.pvs"
  local hdr_file="$data_dir/headers1.txt"
  http_get_config "$port" 2 "$body_file" "$hdr_file"

  local checksum_hdr=$(extract_header "$hdr_file" "X-Config-Checksum")
  local size_hdr=$(extract_header "$hdr_file" "X-Config-Size")
  local version_hdr=$(extract_header "$hdr_file" "X-Config-Version")

  # Verify checksum header matches body
  local body_checksum=$(sha256_file "$body_file")
  assert_eq "$checksum_hdr" "$body_checksum" "Checksum header must match body sha256"

  # Verify size header
  local body_size=$(stat -c%s "$body_file")
  assert_eq "$size_hdr" "$body_size" "Size header must match body size"

  # Verify version header exists (observability only, don't validate value)
  assert_ne "$version_hdr" "" "Version header must be present"

  # Test 2: Timeout returns unchanged LKG (idempotent)
  local body_file2="$data_dir/body2.pvs"
  local hdr_file2="$data_dir/headers2.txt"
  http_get_config "$port" 2 "$body_file2" "$hdr_file2"

  local checksum_hdr2=$(extract_header "$hdr_file2" "X-Config-Checksum")
  assert_eq "$checksum_hdr2" "$checksum_hdr" "Timeout should return same checksum"

  # Verify bodies identical
  local diff_result=$(diff "$body_file" "$body_file2" || echo "different")
  assert_eq "$diff_result" "" "Timeout should return identical body"

  # Test 3: Long-poll wake on publish
  local body_file3="$data_dir/body3.pvs"
  local hdr_file3="$data_dir/headers3.txt"
  local pid_file="$data_dir/longpoll.pid"

  # Start long-poll request in background (30s timeout)
  local start_time=$(date +%s)
  http_get_config_async "$port" 30 "$body_file3" "$hdr_file3" "$pid_file"
  local longpoll_pid=$(cat "$pid_file")

  # Wait 1s, then publish artifact B
  sleep 1
  local artifact_b="$data_dir/test_b.pvs"
  sed 's/basic/modified/g' examples/basic.yaml > /tmp/modified.yaml
  pavctl gen /tmp/modified.yaml "$artifact_b"
  http_publish "$port" "$artifact_b"

  # Wait for long-poll to complete
  wait "$longpoll_pid"
  local end_time=$(date +%s)
  local elapsed=$((end_time - start_time))

  # Verify it woke quickly (< 5s, not full 30s timeout)
  if [ "$elapsed" -gt 5 ]; then
    echo "ERROR: Long-poll took ${elapsed}s, expected < 5s (should wake on publish)"
    exit 1
  fi

  # Verify checksum changed
  local checksum_hdr3=$(extract_header "$hdr_file3" "X-Config-Checksum")
  assert_ne "$checksum_hdr3" "$checksum_hdr" "Checksum should change after publish"

  # Verify new checksum matches artifact B
  local checksum_b=$(sha256_file "$artifact_b")
  assert_eq "$checksum_hdr3" "$checksum_b" "New checksum should match artifact B"

  stop_relay "$relay_pid"
  echo "[TEST 51] PASSED"
}

main "$@"
```

**Tasks:**
- [ ] Implement async HTTP helper for background long-poll
- [ ] Verify header extraction works reliably (case-insensitive)
- [ ] Test timing is robust (doesn't flake on slow CI)
- [ ] Verify body verification logic is correct

---

### 4.3. Test 52: Agent Checksum Deduplication

**File:** `tests/suites/integrated/52_agent_checksum_dedup.sh`

**Coverage:**
- Client change detection via checksum comparison
- Same checksum → skip apply (no-op)
- Different checksum → apply
- Ignores version changes when checksum unchanged

**Implementation:**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/_lib_relay.sh"

# Mini "agent" that tracks last checksum and apply count
agent_poll() {
  local port=$1
  local state_file=$2  # Stores last_checksum
  local apply_count_file=$3

  local body_file=$(mktemp)
  local hdr_file=$(mktemp)

  http_get_config "$port" 2 "$body_file" "$hdr_file"
  local new_checksum=$(extract_header "$hdr_file" "X-Config-Checksum")

  # Verify body matches header checksum
  local body_checksum=$(sha256_file "$body_file")
  assert_eq "$body_checksum" "$new_checksum" "Body checksum must match header"

  # Load last checksum
  local last_checksum=""
  if [ -f "$state_file" ]; then
    last_checksum=$(cat "$state_file")
  fi

  # Dedup logic
  if [ "$new_checksum" != "$last_checksum" ]; then
    # Apply (increment count)
    local count=0
    if [ -f "$apply_count_file" ]; then
      count=$(cat "$apply_count_file")
    fi
    count=$((count + 1))
    echo "$count" > "$apply_count_file"
    echo "$new_checksum" > "$state_file"
    echo "APPLIED (count=$count, checksum=$new_checksum)"
  else
    echo "SKIPPED (checksum unchanged)"
  fi

  rm -f "$body_file" "$hdr_file"
}

main() {
  echo "[TEST 52] Agent checksum deduplication"

  local data_dir=$(mk_tmpdir "test52")
  local port=$(find_free_port)
  local relay_pid=$(start_relay "$data_dir" "$port")

  local state_file="$data_dir/agent_state.txt"
  local apply_count_file="$data_dir/apply_count.txt"

  # Publish artifact A
  local artifact_a="$data_dir/test_a.pvs"
  pavctl gen examples/basic.yaml "$artifact_a"
  http_publish "$port" "$artifact_a"

  # Test 1: First poll → apply (count = 1)
  agent_poll "$port" "$state_file" "$apply_count_file"
  local count=$(cat "$apply_count_file")
  assert_eq "$count" "1" "First poll should apply"

  # Test 2: Second poll (no publish) → skip (count stays 1)
  agent_poll "$port" "$state_file" "$apply_count_file"
  count=$(cat "$apply_count_file")
  assert_eq "$count" "1" "Second poll should skip (checksum unchanged)"

  # Test 3: Publish same artifact again (new version, SAME checksum) → skip
  http_publish "$port" "$artifact_a"
  agent_poll "$port" "$state_file" "$apply_count_file"
  count=$(cat "$apply_count_file")
  assert_eq "$count" "1" "Same artifact republish should skip (checksum unchanged)"

  # Test 4: Publish different artifact B → apply (count = 2)
  local artifact_b="$data_dir/test_b.pvs"
  sed 's/basic/modified/g' examples/basic.yaml > /tmp/modified.yaml
  pavctl gen /tmp/modified.yaml "$artifact_b"
  http_publish "$port" "$artifact_b"

  agent_poll "$port" "$state_file" "$apply_count_file"
  count=$(cat "$apply_count_file")
  assert_eq "$count" "2" "Different artifact should apply"

  # Test 5: Poll again → skip (count stays 2)
  agent_poll "$port" "$state_file" "$apply_count_file"
  count=$(cat "$apply_count_file")
  assert_eq "$count" "2" "Subsequent poll should skip"

  stop_relay "$relay_pid"
  echo "[TEST 52] PASSED"
}

main "$@"
```

**Tasks:**
- [ ] Implement agent_poll function
- [ ] Verify dedup logic matches spec
- [ ] Test handles empty state (first poll)
- [ ] Verify applies exactly when checksum changes

---

### 4.4. Test 53: Restart and State Cache Repair

**File:** `tests/suites/integrated/53_relay_restart_and_state_cache.sh`

**Coverage:**
- LKG metadata is authoritative
- state.json is cache only
- Startup repairs state.json from LKG
- Corrupted/stale state.json does not affect version

**Implementation:**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/_lib_relay.sh"

main() {
  echo "[TEST 53] Restart and state cache repair"

  local data_dir=$(mk_tmpdir "test53")
  local port=$(find_free_port)

  # Start relay, publish A and B
  local relay_pid=$(start_relay "$data_dir" "$port")

  local artifact_a="$data_dir/test_a.pvs"
  local artifact_b="$data_dir/test_b.pvs"
  pavctl gen examples/basic.yaml "$artifact_a"
  sed 's/basic/modified/g' examples/basic.yaml > /tmp/modified.yaml
  pavctl gen /tmp/modified.yaml "$artifact_b"

  http_publish "$port" "$artifact_a"
  http_publish "$port" "$artifact_b"

  # Verify current version is 2
  local status=$(http_status "$port")
  local version=$(json_field "$status" ".current_version")
  assert_eq "$version" "2" "Version should be 2 after two publishes"

  # Stop relay
  stop_relay "$relay_pid"

  # Test 1: Corrupt state.json to version 0
  echo '{"current_version": 0}' > "$data_dir/state.json"

  # Restart relay
  relay_pid=$(start_relay "$data_dir" "$port")

  # Verify version recovered from LKG (should be 2, not 0)
  status=$(http_status "$port")
  version=$(json_field "$status" ".current_version")
  assert_eq "$version" "2" "Version should recover from LKG (2), not stale state.json (0)"

  # Verify state.json was rewritten
  local state_content=$(cat "$data_dir/state.json")
  local state_version=$(json_field "$state_content" ".current_version")
  assert_eq "$state_version" "2" "state.json should be repaired to match LKG"

  # Publish C and verify monotonic increment from LKG version
  local artifact_c="$data_dir/test_c.pvs"
  sed 's/basic/third/g' examples/basic.yaml > /tmp/third.yaml
  pavctl gen /tmp/third.yaml "$artifact_c"
  local resp=$(http_publish "$port" "$artifact_c")
  local new_version=$(json_field "$resp" ".version")
  assert_eq "$new_version" "3" "Next publish should be version 3 (2+1)"

  stop_relay "$relay_pid"

  # Test 2: Corrupt state.json to absurdly high version
  echo '{"current_version": 999}' > "$data_dir/state.json"

  relay_pid=$(start_relay "$data_dir" "$port")

  # Verify version is still 3 from LKG (not 999 from state.json)
  status=$(http_status "$port")
  version=$(json_field "$status" ".current_version")
  assert_eq "$version" "3" "Version should be from LKG (3), not corrupted state.json (999)"

  # Verify state.json repaired again
  state_content=$(cat "$data_dir/state.json")
  state_version=$(json_field "$state_content" ".current_version")
  assert_eq "$state_version" "3" "state.json should be repaired to 3"

  stop_relay "$relay_pid"
  echo "[TEST 53] PASSED"
}

main "$@"
```

**Tasks:**
- [ ] Implement test script
- [ ] Verify state.json format matches spec (minimal: only current_version)
- [ ] Test both lower and higher corrupted values
- [ ] Verify subsequent publishes use LKG version, not corrupted state

---

### 4.5. Test 54: Crash Recovery Matrix

**File:** `tests/suites/integrated/54_relay_crash_recovery_matrix.sh`

**Coverage:**
- All crash scenarios from spec section 1.5
- Failpoint-driven deterministic crashes
- Startup repair procedures
- Recovery from history when possible

**Implementation:**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/_lib_relay.sh"

# Test crash at specific failpoint
test_crash_scenario() {
  local failpoint=$1
  local expected_behavior=$2

  echo "  Testing failpoint: $failpoint"

  local data_dir=$(mk_tmpdir "test54_$failpoint")
  local port=$(find_free_port)

  # Start relay with failpoint enabled
  local relay_pid=$(start_relay_with_failpoint "$data_dir" "$port" "$failpoint")

  # Attempt publish (should crash at failpoint)
  local artifact="$data_dir/test.pvs"
  pavctl gen examples/basic.yaml "$artifact"

  set +e
  http_publish "$port" "$artifact" 2>/dev/null
  local publish_status=$?
  set -e

  # Verify publish failed (relay crashed)
  assert_ne "$publish_status" "0" "Publish should fail (relay crashed at $failpoint)"

  # Wait for relay to exit
  wait "$relay_pid" 2>/dev/null || true

  # Restart relay normally (no failpoint)
  relay_pid=$(start_relay "$data_dir" "$port")

  # Verify expected behavior based on failpoint
  case "$failpoint" in
    after_validation)
      # No files should exist, version 0
      assert_file_not_exists "$data_dir/history/0000000001.pvs"
      assert_file_not_exists "$data_dir/lkg/config.pvs"
      local status=$(http_status "$port")
      local version=$(json_field "$status" ".current_version")
      assert_eq "$version" "0" "Version should be 0 (no publish succeeded)"
      ;;

    after_history_write)
      # History entry exists, LKG absent, version 0, orphan logged
      assert_file_exists "$data_dir/history/0000000001.pvs"
      assert_file_exists "$data_dir/history/0000000001.meta.json"
      assert_file_not_exists "$data_dir/lkg/config.pvs"
      local status=$(http_status "$port")
      local version=$(json_field "$status" ".current_version")
      assert_eq "$version" "0" "Version should be 0 (LKG promotion failed)"
      # TODO: Optionally verify log contains "orphan" warning
      ;;

    after_lkg_artifact_write)
      # Orphaned lkg/config.pvs without meta → repair deletes it
      # OR recovery from history if available
      # After repair, version should be 0 (no complete LKG)
      local status=$(http_status "$port")
      local version=$(json_field "$status" ".current_version")
      # If recovery from history worked, version might be 1
      # If orphan deleted, version should be 0
      # Spec allows both paths, verify one succeeded
      if [ -f "$data_dir/lkg/meta.json" ]; then
        # Recovery succeeded
        assert_file_exists "$data_dir/lkg/config.pvs"
        assert_eq "$version" "1" "Version should be 1 (recovered from history)"
      else
        # Orphan deleted
        assert_file_not_exists "$data_dir/lkg/config.pvs"
        assert_eq "$version" "0" "Version should be 0 (orphan deleted, no recovery)"
      fi
      ;;

    after_lkg_meta_write)
      # LKG complete, state.json stale → version from LKG
      assert_file_exists "$data_dir/lkg/config.pvs"
      assert_file_exists "$data_dir/lkg/meta.json"
      local status=$(http_status "$port")
      local version=$(json_field "$status" ".current_version")
      assert_eq "$version" "1" "Version should be 1 (from LKG)"

      # Verify /v1/config serves valid LKG with correct checksum
      local body_file="$data_dir/body.pvs"
      local hdr_file="$data_dir/headers.txt"
      http_get_config "$port" 2 "$body_file" "$hdr_file"
      local checksum_hdr=$(extract_header "$hdr_file" "X-Config-Checksum")
      local body_checksum=$(sha256_file "$body_file")
      assert_eq "$checksum_hdr" "$body_checksum" "Checksum header must match body"
      ;;

    after_state_write)
      # Normal case, everything consistent
      assert_file_exists "$data_dir/lkg/config.pvs"
      assert_file_exists "$data_dir/lkg/meta.json"
      assert_file_exists "$data_dir/state.json"
      local status=$(http_status "$port")
      local version=$(json_field "$status" ".current_version")
      assert_eq "$version" "1" "Version should be 1"
      ;;
  esac

  stop_relay "$relay_pid"
  echo "  ✓ Failpoint $failpoint behaved correctly"
}

main() {
  echo "[TEST 54] Crash recovery matrix (requires failpoints)"

  # Verify failpoint feature is available
  if ! cargo build --release --features relay-failpoints -p pavis-relay 2>/dev/null; then
    echo "ERROR: Cannot compile relay with failpoints feature"
    exit 1
  fi

  # Test each failpoint scenario
  test_crash_scenario "after_validation" "no files, version 0"
  test_crash_scenario "after_history_write" "orphan history, no LKG"
  test_crash_scenario "after_lkg_artifact_write" "orphan or recovery"
  test_crash_scenario "after_lkg_meta_write" "LKG complete"
  test_crash_scenario "after_state_write" "normal"

  echo "[TEST 54] PASSED"
}

main "$@"
```

**Tasks:**
- [ ] Implement failpoint mechanism in relay
- [ ] Add start_relay_with_failpoint helper
- [ ] Implement all 5 crash scenarios
- [ ] Verify recovery logic for each scenario
- [ ] Ensure deterministic behavior (no timing-based kills)
- [ ] Optional: Capture and verify log warnings for orphans

---

### 4.6. Test 55: History Integrity and Orphans

**File:** `tests/suites/integrated/55_history_integrity_and_orphans.sh`

**Coverage:**
- History file naming format (10-digit zero-padded)
- Orphan detection and safe handling
- Orphans don't affect serving
- Current version derived from LKG, not history

**Implementation:**

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/_lib_relay.sh"

main() {
  echo "[TEST 55] History integrity and orphans"

  local data_dir=$(mk_tmpdir "test55")
  local port=$(find_free_port)
  local relay_pid=$(start_relay "$data_dir" "$port")

  # Publish A, B, C
  local artifact_a="$data_dir/test_a.pvs"
  local artifact_b="$data_dir/test_b.pvs"
  local artifact_c="$data_dir/test_c.pvs"

  pavctl gen examples/basic.yaml "$artifact_a"
  sed 's/basic/modified/g' examples/basic.yaml > /tmp/modified.yaml
  pavctl gen /tmp/modified.yaml "$artifact_b"
  sed 's/basic/third/g' examples/basic.yaml > /tmp/third.yaml
  pavctl gen /tmp/third.yaml "$artifact_c"

  http_publish "$port" "$artifact_a"
  http_publish "$port" "$artifact_b"
  http_publish "$port" "$artifact_c"

  # Test 1: Verify history file naming (10-digit zero-padded)
  assert_file_exists "$data_dir/history/0000000001.pvs"
  assert_file_exists "$data_dir/history/0000000001.meta.json"
  assert_file_exists "$data_dir/history/0000000002.pvs"
  assert_file_exists "$data_dir/history/0000000002.meta.json"
  assert_file_exists "$data_dir/history/0000000003.pvs"
  assert_file_exists "$data_dir/history/0000000003.meta.json"

  # Verify LKG is version 3
  local lkg_meta=$(cat "$data_dir/lkg/meta.json")
  local lkg_ver=$(json_field "$lkg_meta" ".version")
  assert_eq "$lkg_ver" "3" "LKG version should be 3"

  # Test 2: Create orphan history entry (version 9999)
  # Copy version 3 files to simulate unpromoted publish
  cp "$data_dir/history/0000000003.pvs" "$data_dir/history/0000009999.pvs"
  cp "$data_dir/history/0000000003.meta.json" "$data_dir/history/0000009999.meta.json"

  # Edit orphan meta to have version 9999
  local orphan_meta=$(cat "$data_dir/history/0000009999.meta.json")
  orphan_meta=$(echo "$orphan_meta" | jq '.version = 9999')
  echo "$orphan_meta" > "$data_dir/history/0000009999.meta.json"

  stop_relay "$relay_pid"

  # Test 3: Restart relay and verify orphan handling
  relay_pid=$(start_relay "$data_dir" "$port")

  # Current version should still be 3 (from LKG, not orphan)
  local status=$(http_status "$port")
  local version=$(json_field "$status" ".current_version")
  assert_eq "$version" "3" "Version should be 3 from LKG (orphan ignored)"

  # History count might include orphan (implementation-defined)
  # Spec allows orphan to exist harmlessly

  # Test 4: /v1/config serves LKG v3 checksum (orphan doesn't affect)
  local body_file="$data_dir/body.pvs"
  local hdr_file="$data_dir/headers.txt"
  http_get_config "$port" 2 "$body_file" "$hdr_file"

  local checksum_hdr=$(extract_header "$hdr_file" "X-Config-Checksum")
  local lkg_checksum=$(json_field "$lkg_meta" ".checksum")
  assert_eq "$checksum_hdr" "$lkg_checksum" "Config should serve LKG v3"

  # Verify version header is 3 (not 9999)
  local version_hdr=$(extract_header "$hdr_file" "X-Config-Version")
  assert_eq "$version_hdr" "3" "Version header should be 3 (not orphan)"

  # Test 5: Subsequent publish uses LKG version, not orphan
  local artifact_d="$data_dir/test_d.pvs"
  sed 's/basic/fourth/g' examples/basic.yaml > /tmp/fourth.yaml
  pavctl gen /tmp/fourth.yaml "$artifact_d"

  local resp=$(http_publish "$port" "$artifact_d")
  local new_version=$(json_field "$resp" ".version")
  assert_eq "$new_version" "4" "Next publish should be version 4 (3+1, not 9999+1)"

  # Test 6: Verify orphan still exists (not deleted)
  assert_file_exists "$data_dir/history/0000009999.pvs"
  assert_file_exists "$data_dir/history/0000009999.meta.json"

  # Optional: Verify log contains "orphan" warning
  # (requires capturing relay logs - implementation-defined)

  stop_relay "$relay_pid"
  echo "[TEST 55] PASSED"
}

main "$@"
```

**Tasks:**
- [ ] Implement test script
- [ ] Verify orphan creation doesn't corrupt relay
- [ ] Verify version continues from LKG, not orphan
- [ ] Optional: Capture and verify log warnings
- [ ] Test orphans are harmless and don't block serving

---

## 5. CI Integration

### 5.1. Makefile Targets

**Location:** `Makefile` or `make/test.mk`

```makefile
.PHONY: test-e2e-relay-versioning
test-e2e-relay-versioning:
	@echo "Running relay versioning E2E tests..."
	@$(MAKE) test-e2e-relay-versioning-normal
	@$(MAKE) test-e2e-relay-versioning-failpoints

.PHONY: test-e2e-relay-versioning-normal
test-e2e-relay-versioning-normal:
	@echo "Tests without failpoints (50, 51, 52, 53, 55)..."
	@cargo build --release -p pavis-relay
	@cargo build --release -p pavctl
	@./tests/suites/integrated/50_relay_publish_and_versioning.sh
	@./tests/suites/integrated/51_relay_config_longpoll_and_checksum.sh
	@./tests/suites/integrated/52_agent_checksum_dedup.sh
	@./tests/suites/integrated/53_relay_restart_and_state_cache.sh
	@./tests/suites/integrated/55_history_integrity_and_orphans.sh

.PHONY: test-e2e-relay-versioning-failpoints
test-e2e-relay-versioning-failpoints:
	@echo "Test with failpoints (54)..."
	@cargo build --release --features relay-failpoints -p pavis-relay
	@PAVIS_RELAY_BIN=./target/release/pavis-relay \
	  ./tests/suites/integrated/54_relay_crash_recovery_matrix.sh
	@# Rebuild without failpoints for subsequent tests
	@cargo build --release -p pavis-relay

.PHONY: test-e2e
test-e2e: test-e2e-relay-versioning
	@echo "All E2E tests passed"
```

### 5.2. GitHub Actions Workflow

**Location:** `.github/workflows/test-e2e.yml`

```yaml
name: E2E Tests

on:
  push:
    branches: [main]
  pull_request:

jobs:
  relay-versioning:
    name: Relay Versioning E2E
    strategy:
      matrix:
        os: [ubuntu-latest]
        arch: [amd64, arm64]
    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v3

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true

      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y jq curl bc

      - name: Run E2E tests
        run: make test-e2e-relay-versioning
        env:
          RUST_LOG: info

      - name: Upload test artifacts on failure
        if: failure()
        uses: actions/upload-artifact@v3
        with:
          name: relay-test-artifacts-${{ matrix.arch }}
          path: /tmp/pavis_test_*
```

**Tasks:**
- [ ] Add Makefile targets
- [ ] Create GitHub Actions workflow
- [ ] Verify runs on both amd64 and arm64
- [ ] Add artifact upload on failure for debugging

---

## 6. Implementation Checklist

### Phase 1: Infrastructure
- [ ] Create `tests/suites/integrated/_lib_relay.sh` with all helpers
- [ ] Implement `mk_tmpdir` with cleanup traps
- [ ] Implement `start_relay` / `stop_relay` with health checks
- [ ] Implement `http_*` helpers using curl
- [ ] Implement `sha256_file` using sha256sum
- [ ] Implement assertion helpers
- [ ] Test helper library in isolation

### Phase 2: Failpoint Mechanism
- [ ] Add `relay-failpoints` feature to pavis-relay Cargo.toml
- [ ] Create `crates/pavis-relay/src/failpoints.rs`
- [ ] Add failpoint checks to publish handler
- [ ] Verify failpoints trigger correctly (manual test)
- [ ] Verify failpoints are no-op when feature disabled

### Phase 3: Test Implementation
- [ ] Implement test 50 (publish and versioning)
- [ ] Implement test 51 (config long-poll and checksum)
- [ ] Implement test 52 (agent checksum dedup)
- [ ] Implement test 53 (restart and state cache)
- [ ] Implement test 54 (crash recovery matrix)
- [ ] Implement test 55 (history integrity and orphans)

### Phase 4: CI Integration
- [ ] Add Makefile targets
- [ ] Create GitHub Actions workflow
- [ ] Test locally on linux/amd64
- [ ] Test locally on linux/arm64 (if available)
- [ ] Verify CI passes

### Phase 5: Validation
- [ ] All tests pass locally
- [ ] All tests pass in CI
- [ ] Tests are deterministic (no flakes)
- [ ] Tests clean up properly (no leftover files)
- [ ] Coverage verified against spec

---

## 7. Coverage Matrix

| Spec Requirement | Test Coverage |
|-----------------|---------------|
| Monotonic version (v+1) | 50, 53 |
| Version 0 sentinel | 50, 53 |
| Idempotency (same artifact → new version, SAME checksum) | 50 |
| LKG meta authoritative | 53, 54 |
| state.json cache repair | 53 |
| /v1/config checksum headers | 51 |
| X-Config-Checksum == sha256(body) | 51, 52 |
| X-Config-Version observability-only | 51 |
| Long-poll wake on publish | 51 |
| Long-poll timeout idempotent | 51 |
| ConfigAgent checksum dedup | 52 |
| Crash after validation | 54 |
| Crash after history write | 54 |
| Crash after LKG artifact write | 54 |
| Crash after LKG meta write | 54 |
| Crash after state write | 54 |
| History recovery from crash | 54 |
| History file naming (10-digit) | 55 |
| Orphan detection | 55 |
| Orphans safe and ignorable | 54, 55 |
| Orphans don't affect serving | 55 |

**Coverage:** 20/20 spec requirements (100%)

---

## 8. Success Criteria

✅ **Completeness:**
- All 6 E2E tests implemented
- Shared helper library complete
- Failpoint mechanism feature-gated
- CI integration working

✅ **Correctness:**
- All tests verify checksum header == sha256(body)
- Tests verify state.json repair from LKG
- Crash recovery tests are deterministic
- No timing-based race conditions

✅ **Reliability:**
- Tests pass consistently (no flakes)
- Tests clean up temp directories
- Tests don't conflict (isolated ports/data)
- Tests run on both amd64 and arm64

✅ **Maintainability:**
- Shared helpers reduce duplication
- Clear test structure (setup, test, cleanup)
- Assertions have descriptive messages
- Tests match spec requirements 1:1

---

## 9. Future Enhancements (Out of Scope)

- [ ] Concurrent publish stress test
- [ ] Network partition simulation (relay unreachable)
- [ ] Disk full / write failure simulation
- [ ] History retention policy tests (when implemented)
- [ ] Multi-relay HA tests (when implemented)
- [ ] Performance regression tracking

---

**Status:** Draft (Ready for Review)
**Last Updated:** 2026-01-16
