#!/bin/bash
set -e

# Case: robustness_01_subscriber_reconnect
# Category: Robustness
# Invariants: R2 (Versioned), R3 (Efficient Long-Poll)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "robustness_01"
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
	distribution:
	  long_poll:
	    enabled: true
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/v1.pvs" "v1"
gen_minimal_pvs "$TEST_TMP/v2.pvs" "v2"

# 1. Publish V1
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

# 2. Start long-poll (Blocks)
# We use --max-time to simulate disconnect
pavis_curl_body -m 1 -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000" || true

# 3. Publish V2
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/v2.pvs" > /dev/null

# 4. Reconnect
START_TIME=$(date +%s)
pavis_curl_headers "$TEST_TMP/resp" -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000"
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

# 5. Assert
assert_status_eq "$TEST_TMP/resp" 200
pavis_curl_body -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config" > "$TEST_TMP/body"
if ! cmp -s "$TEST_TMP/v2.pvs" "$TEST_TMP/body"; then
    echo "❌ Body mismatch after reconnect"
    exit 1
fi

if [ "$DURATION" -ge 2 ]; then
    echo "❌ Request blocked unexpectedly after reconnect (should be immediate update)"
    exit 1
fi

echo "✅ robustness_01_subscriber_reconnect passed"
