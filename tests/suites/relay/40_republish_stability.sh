#!/bin/bash
set -e

# Case: republish_stability
# Category: Long-Poll Semantics
# Invariants: R3 (Long-Poll), R5 (No False Wake)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "republish_stability"
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
	  service_name: "relay-test-republish"
EOF_INNER

"$PAVCTL_BIN" gen "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

curl -sS -X POST -H "Content-Type: application/octet-stream" \
    --data-binary "@$TEST_TMP/config.pvs" \
    "http://127.0.0.1:$PORT_RELAY/v1/publish"

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers1.txt" "$TEST_TMP/body1.bin"
ETAG1=$(extract_etag "$TEST_TMP/headers1.txt")
assert_etag_format "$ETAG1"

START=$(now_ms)
(
    CODE=$(assert_no_body "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=3000" \
        "$TEST_TMP/headers_longpoll.txt" -H "If-None-Match: $ETAG1")
    echo "$CODE" > "$TEST_TMP/longpoll_result.txt"
) &
LONGPOLL_PID=$!

sleep 0.5

echo "Republishing identical .pvs artifact..."
curl -sS -X POST -H "Content-Type: application/octet-stream" \
    --data-binary "@$TEST_TMP/config.pvs" \
    "http://127.0.0.1:$PORT_RELAY/v1/publish"

sleep 0.5

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers2.txt" "$TEST_TMP/body2.bin"
ETAG2=$(extract_etag "$TEST_TMP/headers2.txt")
assert_eq "$ETAG2" "$ETAG1" "ETag must not change on republish of identical bytes"

wait $LONGPOLL_PID
ELAPSED=$(($(now_ms) - START))

LONGPOLL_CODE=$(cat "$TEST_TMP/longpoll_result.txt")
assert_eq "$LONGPOLL_CODE" "204" "Long-poll should timeout with 204 (no early wake)"

if [ "$ELAPSED" -lt 2800 ] || [ "$ELAPSED" -gt 3300 ]; then
    echo "❌ Long-poll woke early: ${ELAPSED}ms (expected ~3000ms)"
    echo "   This indicates false wakeup on republish!"
    exit 1
fi

echo "✅ Republish stability test passed (no false wakeup)"
