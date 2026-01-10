#!/bin/bash
set -e

# Case: concurrency_01_rapid_publish
# Category: Concurrency
# Invariants: R5 (Concurrency Safety), R2 (Versioned)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "concurrency_01"
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

# Generate 50 payloads
for i in {1..50}; do
    gen_minimal_pvs "$TEST_TMP/payload-$i.pvs" "payload-$i"
done

# 1. Start Publisher Loop
(
    for i in {1..50}; do
        pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
            -H "x-pavis-version: $i" \
            --data-binary "@$TEST_TMP/payload-$i.pvs" >/dev/null || echo "Pub $i failed"
    done
) &
PUB_PID=$!

# 2. Start Subscriber Loop
VERSION="0"
LAST_VER="0"
(
    # Run for approx same duration or until done
    for i in {1..100}; do
        # Short timeout to catch updates fast
        pavis_curl_headers "$TEST_TMP/resp_$i" -H "x-pavis-version: $VERSION" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=100"
        
        if grep -q "200 OK" "$TEST_TMP/resp_$i"; then
            V=$(header_value "$TEST_TMP/resp_$i" "x-pavis-version")
            # Monotonicity check
            if [ "$V" -lt "$LAST_VER" ]; then
                echo "FAIL: Version regression detected: $V < $LAST_VER" > "$TEST_TMP/sub_fail"
                exit 1
            fi
            VERSION=$V
            LAST_VER=$V
        fi
        
        if [ "$LAST_VER" == "50" ]; then
            echo "DONE" > "$TEST_TMP/sub_done"
            break
        fi
        
        # Check if publisher finished and we are stale?
        if ! kill -0 $PUB_PID 2>/dev/null; then
             # Give it one last check
             pavis_curl_headers "$TEST_TMP/final_check" -H "x-pavis-version: $VERSION" "http://127.0.0.1:$PORT_RELAY/v1/config"
             V=$(header_value "$TEST_TMP/final_check" "x-pavis-version")
             if [ "$V" -lt "$LAST_VER" ]; then
                 echo "FAIL: Version regression in final check: $V < $LAST_VER" > "$TEST_TMP/sub_fail"
                 exit 1
             fi
             if [ "$V" == "50" ]; then
                 echo "DONE" > "$TEST_TMP/sub_done"
                 break
             fi
        fi
    done
) &
SUB_PID=$!

wait $PUB_PID
wait $SUB_PID

# 3. Assert
if [ -f "$TEST_TMP/sub_fail" ]; then
    cat "$TEST_TMP/sub_fail"
    exit 1
fi

if [ ! -f "$TEST_TMP/sub_done" ]; then
    pavis_curl_headers "$TEST_TMP/final" -H "x-pavis-version: 0" "http://127.0.0.1:$PORT_RELAY/v1/config"
    V=$(header_value "$TEST_TMP/final" "x-pavis-version")
    if [ "$V" != "50" ]; then
        echo "❌ Final state is $V, expected 50"
        exit 1
    fi
fi

# Check Relay Health
if ! pavis_curl_body -f "http://127.0.0.1:$PORT_RELAY/health" >/dev/null; then
    echo "❌ Relay died"
    exit 1
fi

echo "✅ concurrency_01_rapid_publish passed"