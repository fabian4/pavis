#!/bin/bash
set -e

# Case: 13_contract_transport_integrity
# Category: Contract & Integrity
# Invariants: R1 (Opaque), R4 (Transport Integrity)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "13_contract_transport_integrity"
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
	  service_name: "relay-test-transport"
EOF_INNER

"$PAVCTL_BIN" gen "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

curl -sS -X POST -H "Content-Type: application/octet-stream" \
    --data-binary "@$TEST_TMP/config.pvs" \
    "http://127.0.0.1:$PORT_RELAY/v1/publish"

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers.txt" "$TEST_TMP/body.bin"

CODE=$(extract_status_code "$TEST_TMP/headers.txt")
assert_eq "$CODE" "200" "Should return 200 OK"

echo "Validating required headers..."

CONTENT_TYPE=$(grep -i "^content-type:" "$TEST_TMP/headers.txt" | awk '{print $2}' | tr -d '\r')
assert_eq "$CONTENT_TYPE" "application/octet-stream" "Content-Type must be application/octet-stream"

ETAG=$(extract_etag "$TEST_TMP/headers.txt")
assert_etag_format "$ETAG"

CONFIG_SIZE=$(extract_config_size "$TEST_TMP/headers.txt")
BODY_SIZE=$(stat -f%z "$TEST_TMP/body.bin" 2>/dev/null || stat -c%s "$TEST_TMP/body.bin")
assert_eq "$CONFIG_SIZE" "$BODY_SIZE" "x-config-size must match actual body size"

CACHE_CONTROL=$(grep -i "^cache-control:" "$TEST_TMP/headers.txt" | awk '{print $2}' | tr -d '\r')
assert_eq "$CACHE_CONTROL" "no-store" "Cache-Control must be no-store"

if [ "$BODY_SIZE" -eq 0 ]; then
    echo "❌ Response body is empty (should contain .pvs artifact)"
    exit 1
fi

MAGIC=$(head -c 4 "$TEST_TMP/body.bin" | od -An -tx1 | tr -d ' \n')
EXPECTED_MAGIC="50415653"
if [ "$MAGIC" != "$EXPECTED_MAGIC" ]; then
    echo "❌ Invalid .pvs magic bytes: $MAGIC (expected $EXPECTED_MAGIC)"
    exit 1
fi

echo "✅ Transport integrity test passed"
