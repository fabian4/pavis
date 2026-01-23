#!/bin/bash
set -e

# Case: 11_contract_republish_monotonicity
# Category: Contract & Long-Poll Semantics
# Invariants: R1 (Opaque), R2 (ETag), R3 (Long-Poll), R5 (No False Wake)
#
# This test verifies that republishing identical bytes:
# 1. Increments the configuration version.
# 2. Preserves the ETag (content hash).
# 3. Does NOT wake long-poll requests early.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "11_contract_republish_monotonicity"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF_INNER > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	distribution:
	  long_poll:
	    enabled: true
EOF_INNER

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/payload.pvs" "republish"

# --- Step 1: First Publish ---
echo "Publishing initial artifact (v1)..."
pavis_curl_headers "$TEST_TMP/pub1_resp" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/payload.pvs"
assert_status_eq "$TEST_TMP/pub1_resp" 200

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/sub1_resp" "$TEST_TMP/body1"
assert_status_eq "$TEST_TMP/sub1_resp" 200
ETAG1=$(extract_etag "$TEST_TMP/sub1_resp")
assert_etag_format "$ETAG1"
assert_header_eq "$TEST_TMP/sub1_resp" "x-config-version" "1"

# --- Step 2: Start Long-Poll ---
(
    output=$(curl -sS -D "$TEST_TMP/headers_longpoll.txt" -o /dev/null -w "%{http_code} %{time_total} %{size_download}" \
        -H "If-None-Match: $ETAG1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=3000")
    echo "$output" > "$TEST_TMP/longpoll_result.txt"
) &
LONGPOLL_PID=$!

sleep 0.5

# --- Step 3: Second Publish (Same Bytes) ---
echo "Republishing identical artifact (v2)..."
pavis_curl_headers "$TEST_TMP/pub2_resp" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/payload.pvs"
assert_status_eq "$TEST_TMP/pub2_resp" 200

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/sub2_resp" "$TEST_TMP/body2"
assert_status_eq "$TEST_TMP/sub2_resp" 200
ETAG2=$(extract_etag "$TEST_TMP/sub2_resp")
assert_etag_format "$ETAG2"
assert_header_eq "$TEST_TMP/sub2_resp" "x-config-version" "2"

# --- Step 4: Verify Invariants ---
echo "Verifying ETag stability and version monotonicity..."
assert_eq "$ETAG1" "$ETAG2" "ETag must not change on republish of identical bytes"
if ! cmp -s "$TEST_TMP/payload.pvs" "$TEST_TMP/body2"; then
    fail "Body mismatch after republish"
fi

wait $LONGPOLL_PID
result=$(cat "$TEST_TMP/longpoll_result.txt")
LONGPOLL_CODE=$(echo "$result" | awk '{print $1}')
ELAPSED=$(echo "$result" | awk '{printf "%.0f", $2 * 1000}')
SIZE=$(echo "$result" | awk '{print $3}')
assert_eq "204" "$LONGPOLL_CODE" "Long-poll should timeout with 204 (no early wake)"
if [ "$SIZE" != "0" ]; then
    echo "❌ Response should have no body (size_download=$SIZE)"
    exit 1
fi
if [ "$ELAPSED" -lt 2800 ] || [ "$ELAPSED" -gt 3300 ]; then
    echo "❌ Long-poll woke early: ${ELAPSED}ms (expected ~3000ms)"
    exit 1
fi

echo "✅ contract_republish_monotonicity passed"
