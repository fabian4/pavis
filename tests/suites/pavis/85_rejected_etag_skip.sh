#!/bin/bash
set -e

# Case: 85_rejected_etag_skip
# Category: Control Plane Resilience
# Invariants: Runtime keeps serving LKG when relay serves corrupt artifacts

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "85_rejected_etag_skip"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

export MOCK_RELAY_MODE="corrupt-repeat"
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# Minimal config (no routes)
cat <<-EOF_CONF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "dummy"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 1
	routes: []
	telemetry:
	  service_name: "pavis-test"
EOF_CONF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 10

echo "STEP: assert conditional fetch after rejection"
REQ_URL="http://127.0.0.1:$PORT_RELAY/requests"
MAX_RETRIES=30
for _ in $(seq 1 $MAX_RETRIES); do
    REQUESTS=$(curl -s "$REQ_URL" | tr -d '\r')
    if echo "$REQUESTS" | grep -q '"if_none_match":"'; then
        break
    fi
    sleep 0.2
done

if ! echo "$REQUESTS" | grep -q '"if_none_match":"'; then
    echo "❌ Expected conditional requests after rejecting corrupt artifact"
    echo "$REQUESTS"
    exit 1
fi

echo "✅ 85_rejected_etag_skip passed"
