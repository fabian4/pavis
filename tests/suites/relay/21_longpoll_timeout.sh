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

START_TIME=$(now_ms)
CODE=$(assert_no_body "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=2000" \
    "$TEST_TMP/headers_timeout.txt" -H "If-None-Match: $ETAG")
END_TIME=$(now_ms)
DURATION=$((END_TIME - START_TIME))

if [ "$CODE" != "204" ]; then
    echo "❌ Expected 204, got $CODE"
    exit 1
fi

if [ "$DURATION" -lt 1800 ]; then
    echo "❌ Request returned too early: ${DURATION}ms"
    exit 1
fi

echo "✅ longpoll_02_timeout_no_change passed"
