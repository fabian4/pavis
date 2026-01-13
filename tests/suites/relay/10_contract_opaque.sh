#!/bin/bash
set -e

# Case: contract_01_opaque_publish_subscribe
# Category: Contract & Integrity
# Invariants: R1 (Opaque), R2 (Versioned)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "contract_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

# 1. Start Relay (Real)
cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 2. Generate Valid PVS
gen_minimal_pvs "$TEST_TMP/valid.pvs" "test1"

# 3. Publish
pavis_curl_headers "$TEST_TMP/pub_resp" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/valid.pvs"

assert_status_eq "$TEST_TMP/pub_resp" 200

# 4. Subscribe
pavis_curl_headers "$TEST_TMP/sub_resp" "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 0"

# 5. Assertions
assert_status_eq "$TEST_TMP/sub_resp" 200

# Check Body
pavis_curl_body "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 0" > "$TEST_TMP/body"
if ! cmp -s "$TEST_TMP/valid.pvs" "$TEST_TMP/body"; then
    echo "❌ Body mismatch"
    ls -l "$TEST_TMP/valid.pvs" "$TEST_TMP/body"
    exit 1
fi

# Check Version
assert_header_eq "$TEST_TMP/sub_resp" "x-pavis-version" "1"

echo "✅ contract_01_opaque_publish_subscribe passed"