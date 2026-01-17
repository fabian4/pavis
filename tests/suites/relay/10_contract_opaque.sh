#!/bin/bash
set -e

# Case: contract_01_opaque_publish_subscribe
# Category: Contract & Integrity
# Invariants: R1 (Opaque), R2 (ETag)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "contract_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF_INNER > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
EOF_INNER

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/valid.pvs" "test1"

pavis_curl_headers "$TEST_TMP/pub_resp" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/valid.pvs"

assert_status_eq "$TEST_TMP/pub_resp" 200

fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/sub_resp" "$TEST_TMP/body"

assert_status_eq "$TEST_TMP/sub_resp" 200

ETAG=$(extract_etag "$TEST_TMP/sub_resp")
assert_etag_format "$ETAG"

if ! cmp -s "$TEST_TMP/valid.pvs" "$TEST_TMP/body"; then
    echo "❌ Body mismatch"
    ls -l "$TEST_TMP/valid.pvs" "$TEST_TMP/body"
    exit 1
fi

echo "✅ contract_01_opaque_publish_subscribe passed"
