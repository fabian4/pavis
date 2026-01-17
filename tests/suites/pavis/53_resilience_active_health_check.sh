#!/bin/bash
set -e

# Case: resilience_53_active_health_check
# Category: Resilience
# REASON: Ensure active health checks mark endpoints unhealthy and recover after config update.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "resilience_health_check"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<-EOF > "$TEST_TMP/config_bad.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    health_check:
      path: "/status?code=500"
      interval: "200ms"
      timeout: "200ms"
      healthy_threshold: 1
      unhealthy_threshold: 1
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config_bad.yaml" "$TEST_TMP/config_bad.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_bad.pvs"
run_pavis "$TEST_TMP/config_bad.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

wait_for_unhealthy() {
    local attempts=25
    for _ in $(seq 1 "$attempts"); do
        local code
        code=$(pavis_curl_body -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/echo")
        if [ "$code" != "200" ]; then
            return 0
        fi
        sleep 0.2
    done
    return 1
}

if ! wait_for_unhealthy; then
    echo "❌ Expected endpoint to become unhealthy after failing health checks"
    exit 1
fi

cat <<-EOF > "$TEST_TMP/config_good.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
upstreams:
  - name: "backend"
    health_check:
      path: "/healthz"
      interval: "200ms"
      timeout: "200ms"
      healthy_threshold: 1
      unhealthy_threshold: 1
    endpoints:
      - ip: "127.0.0.1"
        port: ${UPSTREAM_HTTP_PORT_V1}
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF

gen_pvs "$TEST_TMP/config_good.yaml" "$TEST_TMP/config_good.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_good.pvs"

wait_for_healthy() {
    local attempts=25
    for _ in $(seq 1 "$attempts"); do
        local code
        code=$(pavis_curl_body -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT_PAVIS/echo")
        if [ "$code" == "200" ]; then
            return 0
        fi
        sleep 0.2
    done
    return 1
}

if ! wait_for_healthy; then
    echo "❌ Expected endpoint to recover after health check update"
    exit 1
fi

echo "✅ resilience_53_active_health_check passed"
