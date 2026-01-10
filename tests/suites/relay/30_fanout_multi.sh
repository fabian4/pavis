#!/bin/bash
set -e

# Case: fanout_01_multi_subscriber_broadcast
# Category: Fanout
# Invariants: R4 (Fanout Correctness)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "fanout_01"
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

# 1. Publish V1 (ver 1)
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

# 2. Start 5 Subscribers
SUB_PIDS=""
for i in {1..5}; do
    (
        code=$(pavis_curl_body -o /dev/null -w "%{http_code}" --max-time 10 -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000")
        echo "$i:$code" > "$TEST_TMP/sub_$i"
    ) &
    SUB_PIDS="$SUB_PIDS $!"
done

# Wait for subscribers to be registered in metrics
MAX_RETRIES=50
READY=0
for i in $(seq 1 $MAX_RETRIES); do
    WAIT_COUNT=$(pavis_curl_body "http://127.0.0.1:$PORT_RELAY/v1/metrics" | grep "^pavis_relay_longpoll_wait_total" | awk '{print $2}' || echo "0")
    if [ "${WAIT_COUNT:-0}" -ge 5 ]; then
        READY=1
        break
    fi
    sleep 0.1
done

if [ "$READY" -eq 0 ]; then
    echo "❌ Subscribers did not register in time (found $WAIT_COUNT)"
    exit 1
fi

# 3. Publish V2 (ver 2)
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/v2.pvs" > /dev/null

# 4. Wait for subscribers only
wait $SUB_PIDS

# 5. Assert
for i in {1..5}; do
    RES=$(cat "$TEST_TMP/sub_$i")
    if [[ "$RES" != *":200" ]]; then
        echo "❌ Subscriber $i failed: $RES"
        exit 1
    fi
done

echo "✅ fanout_01_multi_subscriber_broadcast passed"