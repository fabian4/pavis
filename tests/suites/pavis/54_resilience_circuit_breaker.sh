#!/bin/bash
set -e

# Case: resilience_54_circuit_breaker
# Category: Resilience
# REASON: Ensure breaker caps reject overflow requests with 503 when in-flight and pending limits are reached.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "resilience_circuit_breaker"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    circuit_breaker:
      max_connections: 1
      max_pending_requests: 1
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

curl -s --max-time 5 -o /dev/null "http://127.0.0.1:$PORT_PAVIS/delay?ms=1500" &
pid1=$!
sleep 0.1
curl -s --max-time 5 -o /dev/null "http://127.0.0.1:$PORT_PAVIS/delay?ms=1500" &
pid2=$!
sleep 0.1

status=$(pavis_curl_body -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/delay?ms=1500")
if [ "$status" != "503" ]; then
    echo "❌ Expected circuit breaker to reject overflow with 503, got $status"
    exit 1
fi

wait "$pid1"
wait "$pid2"

echo "✅ resilience_54_circuit_breaker passed"
