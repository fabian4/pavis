#!/bin/bash
set -e

# Case: robustness_01_subscriber_reconnect
# Category: Robustness
# Invariants: R2 (ETag), R3 (Efficient Long-Poll)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "robustness_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF_INNER > "$TEST_TMP/relay.yaml"
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
EOF_INNER

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/v1.pvs" "v1"
gen_minimal_pvs "$TEST_TMP/v2.pvs" "v2"

pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_v1.txt" "$TEST_TMP/body_v1.bin"
ETAG1=$(extract_etag "$TEST_TMP/headers_v1.txt")
assert_etag_format "$ETAG1"

pavis_curl_body -m 1 -H "If-None-Match: $ETAG1" \
    "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000" || true

pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/v2.pvs" > /dev/null

START_TIME=$(now_ms)
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000" \
    "$TEST_TMP/resp" "$TEST_TMP/body" -H "If-None-Match: $ETAG1"
END_TIME=$(now_ms)
DURATION=$((END_TIME - START_TIME))

assert_status_eq "$TEST_TMP/resp" 200
if ! cmp -s "$TEST_TMP/v2.pvs" "$TEST_TMP/body"; then
    echo "❌ Body mismatch after reconnect"
    exit 1
fi

if [ "$DURATION" -ge 2000 ]; then
    echo "❌ Request blocked unexpectedly after reconnect (should be immediate update)"
    exit 1
fi

echo "✅ robustness_01_subscriber_reconnect passed"
