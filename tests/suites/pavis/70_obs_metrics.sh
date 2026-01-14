#!/bin/bash
set -e

# Case: obs_01_metrics
# Category: Observability
# Invariants: D (Zero-Option)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "obs_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_METRICS=$(get_free_port)
UPSTREAM_PORT=8081

# 1. Config with Metrics
cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: $UPSTREAM_PORT
routes:
  - host: "*"
    paths:
      - matcher: !prefix
          path: "/echo"
        destinations:
          - upstream: "backend"
            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 2. Start Pavis
run_pavis "$TEST_TMP/config.pvs" ""

# 3. Wait for boot
wait_for_port "$PORT_PAVIS" 5
wait_for_port "$PORT_METRICS" 5

# 4. Generate Traffic
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo"
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo?foo=bar"

# 5. Scrape Metrics
METRICS_OUT="$TEST_TMP/metrics.txt"
curl -s "http://127.0.0.1:$PORT_METRICS" > "$METRICS_OUT"

# 6. Assertions
if ! grep -q 'pavis_http_requests_total{method="GET",route="/echo",status="200",upstream="backend"} 2' "$METRICS_OUT"; then
    echo "❌ Metrics missing or incorrect count for http_requests_total"
    cat "$METRICS_OUT"
    exit 1
fi

if ! grep -q 'pavis_upstream_requests_total{upstream="backend",status="200"} 2' "$METRICS_OUT"; then
    echo "❌ Metrics missing or incorrect count for upstream_requests_total"
    cat "$METRICS_OUT"
    exit 1
fi

# 7. Cardinality guardrails
UNMATCHED_A="/random/$(date +%s)"
UNMATCHED_B="/another/$(date +%s%N)"
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS${UNMATCHED_A}"
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS${UNMATCHED_B}"

METRICS_CARD="$TEST_TMP/metrics_cardinality.txt"
curl -s "http://127.0.0.1:$PORT_METRICS" > "$METRICS_CARD"

if grep -q "$UNMATCHED_A" "$METRICS_CARD" || grep -q "$UNMATCHED_B" "$METRICS_CARD"; then
    echo "❌ Cardinality leak detected in metrics"
    cat "$METRICS_CARD"
    exit 1
fi

# 8. Hot Reload and Verify Persistence
PORT_RELAY=$(get_free_port)
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V2: Same config, just to trigger a reload
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Give some time for reload
sleep 2

# Generate more traffic
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo"

# Scrape Metrics again
METRICS_RELOAD="$TEST_TMP/metrics_reload.txt"
curl -s "http://127.0.0.1:$PORT_METRICS" > "$METRICS_RELOAD"

# Invariant A: No-Drop (including metrics state)
if ! grep -q 'pavis_http_requests_total{method="GET",route="/echo",status="200",upstream="backend"} 3' "$METRICS_RELOAD"; then
    echo "❌ Metrics reset or lost during hot-reload"
    grep "pavis_http_requests_total" "$METRICS_RELOAD"
    exit 1
fi

echo "✅ obs_01_metrics passed"
