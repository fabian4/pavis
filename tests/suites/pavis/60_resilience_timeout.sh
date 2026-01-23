#!/bin/bash
set -e

# Case: 60_resilience_timeout
# Category: Resilience
# REASON: Ensure route timeout tightening takes effect after reload.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "60_resilience_timeout"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V1: generous timeout, delay should succeed.
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        timeout: "500ms"
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

assert_status "http://127.0.0.1:$PORT_PAVIS/delay?ms=100" "200"

# V2: tighten timeout; same delay should now fail quickly.
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        timeout: "50ms"
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

MAX_RETRIES=20
SWITCHED=0
attempt=0
for attempt in $(seq 1 $MAX_RETRIES); do
    output=$(pavis_curl_body -o /dev/null -w "%{http_code} %{time_total}" --max-time 2 \
        "http://127.0.0.1:$PORT_PAVIS/delay?ms=200")
    status=$(echo "$output" | awk '{print $1}')
    elapsed_ms=$(echo "$output" | awk '{printf "%.0f", $2 * 1000}')

    if [ "$status" != "200" ] && [ "$elapsed_ms" -lt 500 ]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

assert_retry_succeeded "$attempt" "$MAX_RETRIES"

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Expected tightened timeout to fail quickly after reload"
    exit 1
fi

echo "✅ resilience_50_timeout passed"
