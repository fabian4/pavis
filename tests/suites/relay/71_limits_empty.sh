#!/bin/bash
set -e

# Case: limits_02_empty_publish
# Category: Limits
# Invariants: R1 (Opaque)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "limits_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	source:
	  type: none
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 1. Publish Empty
touch "$TEST_TMP/empty"
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/empty")

# 2. Assert (Empty body is invalid PVS, so 400 or 422)
if [ "$CODE" == "400" ] || [ "$CODE" == "422" ]; then
    # Correct behavior (rejected)
    true
elif [ "$CODE" == "200" ]; then
    echo "❌ Unexpected success for empty body"
    exit 1
else
    echo "❌ Unexpected code $CODE"
    exit 1
fi

echo "✅ limits_02_empty_publish passed"
