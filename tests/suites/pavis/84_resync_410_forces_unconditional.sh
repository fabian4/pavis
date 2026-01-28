#!/bin/bash
set -e

# Case: 84_resync_410_forces_unconditional
# Category: Control Plane Resync
# Invariants: Runtime recovers after 410 and continues polling

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "84_resync_410_forces_unconditional"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

export MOCK_RELAY_MODE="resync-once"
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# Minimal config (no routes)
cat <<-EOF_CONF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "dummy"
	    endpoints: []
	routes: []
	telemetry:
	  service_name: "pavis-test"
EOF_CONF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 10

echo "STEP: assert requests are unconditional after 410"
REQ_URL="http://127.0.0.1:$PORT_RELAY/requests"
MAX_RETRIES=20
for _ in $(seq 1 $MAX_RETRIES); do
    REQUESTS=$(curl -s "$REQ_URL" | tr -d '\r')
    COUNT=$(echo "$REQUESTS" | grep -o '"wait_ms"' | wc -l | tr -d ' ')
    if [ "$COUNT" -ge 2 ]; then
        break
    fi
    sleep 0.2
done

if [ "${COUNT:-0}" -lt 2 ]; then
    echo "❌ Expected at least 2 long-poll requests"
    echo "$REQUESTS"
    exit 1
fi

IF_MATCHES=$(echo "$REQUESTS" | grep -o '"if_none_match":[^,}]*')
FIRST_IF=$(echo "$IF_MATCHES" | sed -n '1p')
SECOND_IF=$(echo "$IF_MATCHES" | sed -n '2p')
if [ "$FIRST_IF" != '"if_none_match":null' ] || [ "$SECOND_IF" != '"if_none_match":null' ]; then
    echo "❌ Expected first two fetches to be unconditional"
    echo "$REQUESTS"
    exit 1
fi

if [ "$(echo "$REQUESTS" | grep -o '"wait_ms":30000' | wc -l | tr -d ' ')" -lt 2 ]; then
    echo "❌ Expected wait_ms=30000 on long-poll requests"
    echo "$REQUESTS"
    exit 1
fi

echo "✅ 84_resync_410_forces_unconditional passed"
