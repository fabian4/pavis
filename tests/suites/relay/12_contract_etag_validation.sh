#!/bin/bash
set -e

# Case: 12_contract_etag_validation
# Category: Long-Poll Semantics
# Invariants: R2 (ETag Validation)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "12_contract_etag_validation"
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
	  service_name: "relay-test-etag"
EOF_INNER

"$PAVCTL_BIN" gen "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
"$PAVCTL_BIN" publish --relay "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"

echo "Testing weak ETag rejection (W/\"...\")..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_weak.txt" "$TEST_TMP/body_weak.bin" \
    -H 'If-None-Match: W/"sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"'
CODE=$(extract_status_code "$TEST_TMP/headers_weak.txt")
assert_eq "$CODE" "200" "Weak ETag should be ignored (unconditional GET)"

echo "Testing wildcard rejection (*)..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_wildcard.txt" "$TEST_TMP/body_wildcard.bin" \
    -H 'If-None-Match: *'
CODE=$(extract_status_code "$TEST_TMP/headers_wildcard.txt")
assert_eq "$CODE" "200" "Wildcard should be ignored (unconditional GET)"

echo "Testing multiple ETags rejection..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_multiple.txt" "$TEST_TMP/body_multiple.bin" \
    -H 'If-None-Match: "etag1", "etag2"'
CODE=$(extract_status_code "$TEST_TMP/headers_multiple.txt")
assert_eq "$CODE" "200" "Multiple ETags should be ignored (unconditional GET)"

echo "Testing malformed ETag (wrong prefix)..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_malformed.txt" "$TEST_TMP/body_malformed.bin" \
    -H 'If-None-Match: "md5:abc123"'
CODE=$(extract_status_code "$TEST_TMP/headers_malformed.txt")
assert_eq "$CODE" "200" "Non-sha256 ETag should be ignored (unconditional GET)"

echo "Testing malformed ETag (wrong hex length)..."
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/headers_short.txt" "$TEST_TMP/body_short.bin" \
    -H 'If-None-Match: "sha256:abc123"'
CODE=$(extract_status_code "$TEST_TMP/headers_short.txt")
assert_eq "$CODE" "200" "Short hex ETag should be ignored (unconditional GET)"

echo "✅ ETag validation test passed"
