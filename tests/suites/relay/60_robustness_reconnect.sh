#!/bin/bash
set -e

# Case: robustness_01_subscriber_reconnect
# Category: Robustness
# Invariants: R2 (Versioned), R3 (Efficient Long-Poll)

source "$(dirname "$0")/../../lib/env.sh"
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
	distribution:
	  long_poll:
	    enabled: true
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

gen_minimal_pvs "$TEST_TMP/v1.pvs" "v1"
gen_minimal_pvs "$TEST_TMP/v2.pvs" "v2"

# 1. Publish V1
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

# 2. Subscribe and Abort (Simulated)
curl -s -m 1 -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000" || true

# 3. Publish V2
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/v2.pvs" > /dev/null

# 4. Reconnect with OLD Version (1)
START=$(date +%s)
RESP=$(curl -s -i -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000")
END=$(date +%s)
DURATION=$((END - START))

if echo "$RESP" | grep -q "200 OK"; then
    # Verify content
    curl -s "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 1" > "$TEST_TMP/body"
    if ! cmp -s "$TEST_TMP/v2.pvs" "$TEST_TMP/body"; then
        echo "❌ Expected v2, got something else"
        exit 1
    fi
else
    echo "❌ Expected 200 OK"
    exit 1
fi

if [ "$DURATION" -ge 2 ]; then echo "❌ Request blocked unexpectedly"; exit 1; fi

echo "✅ robustness_01_subscriber_reconnect passed"