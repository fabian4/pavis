#!/bin/bash
set -e

# Case: 71_limits_empty
# Category: Limits
# Invariants: R1 (Opaque)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "71_limits_empty"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	pipeline:
	  ingest:
	    source:
	      kind: none
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 1. Publish Empty
touch "$TEST_TMP/empty"
pavis_curl_headers "$TEST_TMP/resp" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/empty"

# 2. Assert (Empty body is invalid PVS, so 400 or 422)
CODE=$(head -n 1 "$TEST_TMP/resp" | awk '{print $2}')
if [ "$CODE" == "400" ] || [ "$CODE" == "422" ]; then
    # Correct behavior (rejected)
    true
elif [ "$CODE" == "200" ]; then
    echo "❌ Unexpected success for empty body"
    exit 1
else
    echo "❌ Unexpected status code $CODE"
    exit 1
fi

echo "✅ limits_02_empty_publish passed"
