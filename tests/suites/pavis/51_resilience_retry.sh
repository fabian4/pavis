#!/bin/bash
set -e

# Case: resilience_51_retry_policy
# Category: Resilience
# REASON: Ensure retry_on connect failure succeeds with a fallback endpoint.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "resilience_retry"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
DEAD_PORT=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    balancer: "round-robin"
    pool:
      connect: "100ms"
    endpoints:
      - ip: "127.0.0.1"
        port: ${DEAD_PORT}
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        retry:
          attempts: 2
          per_try_timeout: "200ms"
          retry_on: ["connect_error"]
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
echo "$response" | assert_json_has_key "instance_id"
instance=$(echo "$response" | json_get_string "instance_id")

if [ "$instance" != "backend-v1" ]; then
    echo "❌ Expected retry to reach backend-v1, got $instance"
    exit 1
fi

echo "✅ resilience_51_retry_policy passed"
