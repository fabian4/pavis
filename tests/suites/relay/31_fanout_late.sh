#!/bin/bash
set -e

# Case: fanout_02_catch_up
# Category: Fanout
# Invariants: R2 (ETag Delivery)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "fanout_02"
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
gen_minimal_pvs "$TEST_TMP/v3.pvs" "v3"
gen_minimal_pvs "$TEST_TMP/v4.pvs" "v4"
gen_minimal_pvs "$TEST_TMP/v5.pvs" "v5"

pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_v1.txt" "$TEST_TMP/body_v1.bin"
ETAG1=$(extract_etag "$TEST_TMP/headers_v1.txt")
assert_etag_format "$ETAG1"

pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/v2.pvs" > /dev/null
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/v3.pvs" > /dev/null
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/v4.pvs" > /dev/null
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/v5.pvs" > /dev/null

START=$(now_ms)
CODE=$(pavis_curl_body -o /dev/null -w "%{http_code}" \
    -H "If-None-Match: $ETAG1" \
    "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000")
END=$(now_ms)
DURATION=$((END - START))

if [ "$CODE" != "200" ]; then echo "❌ Expected 200, got $CODE"; exit 1; fi
if [ "$DURATION" -ge 2000 ]; then echo "❌ Request blocked unexpectedly"; exit 1; fi

echo "✅ fanout_02_catch_up passed"
