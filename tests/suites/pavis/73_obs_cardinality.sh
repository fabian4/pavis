#!/bin/bash
set -e

# Case: obs_04_cardinality
# Category: Observability
# Invariants: D (Zero-Option)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "obs_04"
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
# Match: /echo
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo"

# No Match: /random/xyz (404)
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/random/$(date +%s)"
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/another/random/path"

# 5. Scrape Metrics
METRICS_OUT="$TEST_TMP/metrics.txt"
curl -s "http://127.0.0.1:$PORT_METRICS" > "$METRICS_OUT"

# 6. Assertions

# A. Matched request should exist
if ! grep -q 'pavis_http_requests_total{method="GET",route="/echo",status="200",upstream="backend"} 1' "$METRICS_OUT"; then
    echo "❌ Metrics missing matched request"
    cat "$METRICS_OUT"
    exit 1
fi

# B. Unmatched requests must NOT have their path in 'route' label
# We look for the random paths. They should NOT exist.
if grep -q "/random/" "$METRICS_OUT"; then
    echo "❌ Cardinality leak! Found raw path '/random/' in metrics"
    exit 1
fi
if grep -q "/another/" "$METRICS_OUT"; then
    echo "❌ Cardinality leak! Found raw path '/another/' in metrics"
    exit 1
fi

# C. Drop counter should be incremented (2 unmatched requests)
if ! grep -q 'pavis_telemetry_metrics_label_dropped_total 2' "$METRICS_OUT"; then
    echo "❌ Drop counter incorrect or missing. Expected 2."
    grep "pavis_telemetry_metrics_label_dropped_total" "$METRICS_OUT" || echo "(Metric not found)"
    exit 1
fi

echo "✅ obs_04_cardinality passed"
