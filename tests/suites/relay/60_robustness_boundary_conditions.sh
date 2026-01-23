#!/bin/bash
set -e

# Case: 60_robustness_boundary_conditions
# Category: Long-Poll Semantics
# Invariants: R3 (Long-Poll), R2 (ETag Validation)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "60_robustness_boundary_conditions"
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
	  service_name: "relay-test-boundary"
EOF_INNER

"$PAVCTL_BIN" gen "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
"$PAVCTL_BIN" publish --relay "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"

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
output=$(curl -sS -D "$TEST_TMP/headers3.txt" -o "$TEST_TMP/body3.bin" -w "%{http_code} %{time_total}" \
    "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000")
CODE=$(echo "$output" | awk '{print $1}')
ELAPSED=$(echo "$output" | awk '{printf "%.0f", $2 * 1000}')

assert_eq "$CODE" "200" "Missing If-None-Match + wait_ms should return 200"

if [ "$ELAPSED" -gt 500 ]; then
    echo "❌ Request took ${ELAPSED}ms (expected immediate return < 500ms)"
    echo "   Per spec recommendation: long-poll without If-None-Match"
    echo "   should be treated as unconditional GET (return immediately)"
    exit 1
fi

echo "Test 4: wait_ms=60000 (max) with matching ETag should timeout with 204"
echo "NOTE: This test takes 60 seconds. Consider running only in full CI (not fast CI)."

if [ "${CI_PROFILE:-full}" = "fast" ]; then
    echo "⏭️  Skipping 60s test in fast CI mode"
else
    output=$(curl -sS -D "$TEST_TMP/headers4.txt" -o /dev/null -w "%{http_code} %{time_total} %{size_download}" \
        -H "If-None-Match: $ETAG" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=60000")
    CODE=$(echo "$output" | awk '{print $1}')
    ELAPSED=$(echo "$output" | awk '{printf "%.0f", $2 * 1000}')
    SIZE=$(echo "$output" | awk '{print $3}')

    assert_eq "$CODE" "204" "wait_ms=60000 should timeout with 204"
    if [ "$SIZE" != "0" ]; then
        echo "❌ Response should have no body (size_download=$SIZE)"
        exit 1
    fi

    if [ "$ELAPSED" -lt 59000 ] || [ "$ELAPSED" -gt 61000 ]; then
        echo "❌ Timeout incorrect: ${ELAPSED}ms (expected ~60000ms)"
        exit 1
    fi
fi

echo "✅ Boundary conditions test passed"
