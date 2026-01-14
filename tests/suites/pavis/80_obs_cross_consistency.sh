#!/bin/bash
set -e

# Case: obs_80_cross_consistency
# Category: Observability
# Invariants: D (Zero-Option)
# Description: Verify that Metrics, Access Logs, and Response Headers all report consistent values for the same request.

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "obs_80"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_METRICS=$(get_free_port)
ACCESS_LOG_PATH="$TEST_TMP/access.log"
UPSTREAM_PORT=8081

# 1. Config with all signals
cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
  access_log: "$ACCESS_LOG_PATH"
  tracing:
    provider: "otlp"
    endpoint: "http://127.0.0.1:4317"
    sampling: 100
upstreams:
  - name: "backend-consistent"
    endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_PORT }]
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/consistent" }
        destinations: [{ upstream: "backend-consistent", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 2. Start Pavis
run_pavis "$TEST_TMP/config.pvs" ""
wait_for_port "$PORT_PAVIS" 5
wait_for_port "$PORT_METRICS" 5

# 3. Single Tagged Request
TEST_RUN_ID="consistent-$(date +%s)"
RESPONSE=$(pavis_curl_body -H "X-Pavis-Test-Run: $TEST_RUN_ID" "http://127.0.0.1:$PORT_PAVIS/consistent")

# 4. Extract Correlation IDs
TRACE_ID=$(echo "$RESPONSE" | sed -n 's/.*"traceparent":"00-\([0-9a-f]\{32\}\).*/\1/p')
if [ -z "$TRACE_ID" ]; then
    TRACE_ID="NOT_FOUND"
fi
if [ "$TRACE_ID" == "NOT_FOUND" ]; then
    echo "❌ Trace ID not found in response headers"
    exit 1
fi

# 5. Verify Access Log Consistency
# Give a moment for log flush
sleep 1
LOG_LINE=$(grep "$TEST_RUN_ID" "$ACCESS_LOG_PATH" || echo "NOT_FOUND")
if [ "$LOG_LINE" == "NOT_FOUND" ]; then
    echo "❌ Request not found in access log"
    exit 1
fi

echo "$LOG_LINE" | assert_json_has_key "upstream"
LOG_UPSTREAM=$(echo "$LOG_LINE" | python3 -c "import sys, json; print(json.load(sys.stdin)['upstream'])")
if [ "$LOG_UPSTREAM" != "backend-consistent" ]; then
    echo "❌ Access log upstream mismatch: $LOG_UPSTREAM"
    exit 1
fi

# 6. Verify Metrics Consistency
METRICS_OUT="$TEST_TMP/metrics.txt"
curl -s "http://127.0.0.1:$PORT_METRICS" > "$METRICS_OUT"
if ! grep -q 'pavis_http_requests_total{.*upstream="backend-consistent".*} 1' "$METRICS_OUT"; then
    echo "❌ Metrics upstream mismatch or missing"
    grep "pavis_http_requests_total" "$METRICS_OUT"
    exit 1
fi

echo "✅ obs_80_cross_consistency passed"
