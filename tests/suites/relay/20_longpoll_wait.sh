#!/bin/bash
set -e

# Case: longpoll_01_wait_for_update
# Category: Long-Poll Semantics
# Invariants: R3 (Efficient Long-Poll), R2 (Versioned)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "longpoll_01"
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

cat <<-EOF_INNER > "$TEST_TMP/config.yaml"
	version: 1
	listeners:
	  - name: listener
	    address: "127.0.0.1:0"
	upstreams:
	  - name: backend
	    endpoints:
	      - address: "127.0.0.1"
	        port: 8080
	routes: []
	telemetry:
	  service_name: "relay-test-longpoll"
EOF_INNER

"$PAVCTL_BIN" gen "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
"$PAVCTL_BIN" publish --relay "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" "$TEST_TMP/headers1.txt" "$TEST_TMP/body1.bin"
CODE=$(extract_status_code "$TEST_TMP/headers1.txt")
assert_eq "$CODE" "200" "Initial fetch should return 200"

ETAG1=$(extract_etag "$TEST_TMP/headers1.txt")
assert_etag_format "$ETAG1"

LONGPOLL_HEADERS="$TEST_TMP/headers_live.txt"
pavis_curl_headers "$LONGPOLL_HEADERS" \
    -H "If-None-Match: $ETAG1" \
    "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=1000" &
LONGPOLL_PID=$!

sleep 0.1
if ! kill -0 "$LONGPOLL_PID" 2>/dev/null; then
    echo "❌ Long-poll request exited early"
    exit 1
fi

wait "$LONGPOLL_PID"
LIVE_CODE=$(extract_status_code "$LONGPOLL_HEADERS")
assert_eq "$LIVE_CODE" "204" "Long-poll should stay open and timeout with 204"

output=$(curl -sS -D "$TEST_TMP/headers2.txt" -o /dev/null -w "%{http_code} %{time_total} %{size_download}" \
    -H "If-None-Match: $ETAG1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=500")
CODE=$(echo "$output" | awk '{print $1}')
ELAPSED=$(echo "$output" | awk '{printf "%.0f", $2 * 1000}')
SIZE=$(echo "$output" | awk '{print $3}')
if [ "$SIZE" != "0" ]; then
    echo "❌ Response should have no body (size_download=$SIZE)"
    exit 1
fi

assert_eq "$CODE" "204" "Long-poll timeout should return 204"

ETAG2=$(extract_etag "$TEST_TMP/headers2.txt")
assert_eq "$ETAG2" "$ETAG1" "ETag should be unchanged on 204"

if [ "$ELAPSED" -lt 400 ] || [ "$ELAPSED" -gt 700 ]; then
    echo "❌ Long-poll timing incorrect: ${ELAPSED}ms (expected ~500ms)"
    exit 1
fi

echo "✅ longpoll_01_wait_for_update passed"
