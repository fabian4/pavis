#!/bin/bash
set -e

# Case: longpoll_02_timeout_no_change
# Category: Long-Poll Semantics
# Invariants: R3 (Efficient Long-Poll)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "longpoll_02"
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

pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_init.txt" "$TEST_TMP/body_init.bin"
ETAG=$(extract_etag "$TEST_TMP/headers_init.txt")
assert_etag_format "$ETAG"

output=$(curl -sS -D "$TEST_TMP/headers_timeout.txt" -o /dev/null -w "%{http_code} %{time_total} %{size_download}" \
    -H "If-None-Match: $ETAG" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=2000")
CODE=$(echo "$output" | awk '{print $1}')
DURATION=$(echo "$output" | awk '{printf "%.0f", $2 * 1000}')
SIZE=$(echo "$output" | awk '{print $3}')
if [ "$SIZE" != "0" ]; then
    echo "❌ Response should have no body (size_download=$SIZE)"
    exit 1
fi

if [ "$CODE" != "204" ]; then
    echo "❌ Expected 204, got $CODE"
    exit 1
fi

if [ "$DURATION" -lt 1800 ]; then
    echo "❌ Request returned too early: ${DURATION}ms"
    exit 1
fi

echo "✅ longpoll_02_timeout_no_change passed"
