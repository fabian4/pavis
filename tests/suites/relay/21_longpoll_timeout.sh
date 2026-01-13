#!/bin/bash
set -e

# Case: longpoll_02_timeout_no_change
# Category: Long-Poll Semantics
# Invariants: R3 (Efficient Long-Poll)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "longpoll_02"
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

# 1. Publish V1 (ver 1)
pavis_curl_body -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/v1.pvs" > /dev/null

# 2. Start Subscriber
START_TIME=$(date +%s)
# 2000ms timeout
CODE=$(pavis_curl_body -o /dev/null -w "%{http_code}" -H "x-pavis-version: 1" "http://127.0.0.1:$PORT_RELAY/v1/config?wait_ms=2000")
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

# 3. Assert
if [ "$CODE" != "304" ]; then
    echo "❌ Expected 304, got $CODE"
    exit 1
fi

if [ "$DURATION" -lt 2 ]; then
    echo "❌ Request returned too early: $DURATION"
    exit 1
fi

echo "✅ longpoll_02_timeout_no_change passed"