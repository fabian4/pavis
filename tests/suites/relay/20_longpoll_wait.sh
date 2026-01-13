#!/bin/bash
set -e

# Case: longpoll_01_wait_for_update
# Category: Long-Poll Semantics
# Invariants: R3 (Efficient Long-Poll), R2 (Versioned)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "longpoll_01"
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

# 1. Publish V1 (ver 1)
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

# 2. Start Subscriber (Background)
(
    # Request version 1, expect wait.
    code=$(pavis_curl_body -s -o "$TEST_TMP/sub_body" -w "%{http_code}" -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000")
    
    if [ "$code" != "200" ]; then
        echo "FAIL: Code $code" > "$TEST_TMP/result"
    else
        # Verify body matches V2
        if cmp -s "$TEST_TMP/v2.pvs" "$TEST_TMP/sub_body"; then
            echo "PASS" > "$TEST_TMP/result"
        else
            echo "FAIL: Body mismatch" > "$TEST_TMP/result"
        fi
    fi
) &
PID_SUB=$!

# 3. Verify blocking (Subscriber should be alive)
sleep 0.5
if ! kill -0 $PID_SUB 2>/dev/null; then
    echo "❌ Subscriber exited prematurely (did not block)"
    exit 1
fi

# 4. Publish V2 (ver 2) to unblock
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/v2.pvs" > /dev/null

# 5. Wait for subscriber
wait $PID_SUB

# 6. Assert
if [ ! -f "$TEST_TMP/result" ]; then
    echo "❌ Subscriber did not produce result"
    exit 1
fi
RESULT=$(cat "$TEST_TMP/result")
if [ "$RESULT" != "PASS" ]; then
    echo "❌ Subscriber failed: $RESULT"
    exit 1
fi

if [ "$DURATION" -gt 4 ]; then
    echo "❌ Request took too long: $DURATION"
    exit 1
fi

echo "✅ longpoll_01_wait_for_update passed"