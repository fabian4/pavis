#!/bin/bash
set -e

# Case: resilience_52_outlier_detection
# Category: Resilience
# REASON: Validate passive outlier ejection on consecutive 5xx and re-admission after eject_duration.

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "resilience_outlier"
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
    outlier_detection:
      consecutive_errors: 2
      eject_duration: "500ms"
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

assert_status "http://127.0.0.1:$PORT_PAVIS/echo" "200"

assert_status "http://127.0.0.1:$PORT_PAVIS/status?code=500" "500"
assert_status "http://127.0.0.1:$PORT_PAVIS/status?code=500" "500"

status=$(pavis_curl_body -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/echo")
if [ "$status" == "200" ]; then
    echo "❌ Expected endpoint ejection after consecutive failures"
    exit 1
fi

sleep 0.6
assert_status "http://127.0.0.1:$PORT_PAVIS/echo" "200"

echo "✅ resilience_52_outlier_detection passed"
