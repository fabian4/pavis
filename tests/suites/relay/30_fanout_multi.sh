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
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

# 2. Start 5 Subscribers
SUB_PIDS=""
for i in {1..5}; do
    (
        code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=5000")
        echo "$i:$code" > "$TEST_TMP/sub_$i"
    ) &
    SUB_PIDS="$SUB_PIDS $!"
done

# Wait for them to be ready (approx)
sleep 2

# 3. Publish V2 (ver 2)
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
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