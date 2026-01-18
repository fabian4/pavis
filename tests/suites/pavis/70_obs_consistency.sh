#!/bin/bash
set -e

# Case: obs_consistency
# Category: Observability
# Invariants: D (Zero-Option)
# Description: Validate metrics, access logs, tracing, and reload persistence using a single request identity.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "obs_consistency"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_METRICS=$(get_free_port)
PORT_RELAY=$(get_free_port)
ACCESS_LOG_PATH="$TEST_TMP/access.log"
UPSTREAM_PORT=${UPSTREAM_HTTP_PORT_V1}

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

cat <<EOF > "$TEST_TMP/config_v1.yaml"
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
    endpoints:
      - ip: "127.0.0.1"
        port: $UPSTREAM_PORT
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/echo" }
        destinations: [{ upstream: "backend-consistent", weight: 1 }]
      - matcher: !prefix { path: "/consistent" }
        destinations: [{ upstream: "backend-consistent", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

cat <<EOF > "$TEST_TMP/config_v2.yaml"
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
    endpoints:
      - ip: "127.0.0.1"
        port: $UPSTREAM_PORT
routes:
  - host: "*"
    paths:
      - matcher: !prefix { path: "/echo" }
        destinations: [{ upstream: "backend-consistent", weight: 1 }]
      - matcher: !prefix { path: "/consistent" }
        destinations: [{ upstream: "backend-consistent", weight: 1 }]
      - matcher: !prefix { path: "/noop" }
        destinations: [{ upstream: "backend-consistent", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"

cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo"
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo?foo=bar"

RESPONSE=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/consistent")
if ! echo "$RESPONSE" | python3 -c "import sys, json; data=json.load(sys.stdin); assert 'traceparent' in data.get('headers', {})"; then
    echo "❌ Traceparent header missing in upstream request"
    exit 1
fi

MAX_RETRIES=20
LOG_LINE=""
for _ in $(seq 1 $MAX_RETRIES); do
    if [ -f "$ACCESS_LOG_PATH" ]; then
        LOG_LINE=$(grep '"path":"/consistent"' "$ACCESS_LOG_PATH" | tail -n 1)
        if [ -n "$LOG_LINE" ]; then
            break
        fi
    fi
    sleep 0.5
done

if [ -z "$LOG_LINE" ]; then
    echo "❌ Request not found in access log"
    if [ -f "$ACCESS_LOG_PATH" ]; then
        tail -n 20 "$ACCESS_LOG_PATH"
    fi
    exit 1
fi

echo "$LOG_LINE" | assert_json_has_key "upstream"
LOG_UPSTREAM=$(echo "$LOG_LINE" | python3 -c "import sys, json; print(json.load(sys.stdin).get('upstream',''))")
if [ "$LOG_UPSTREAM" != "backend-consistent" ]; then
    echo "❌ Access log upstream mismatch: $LOG_UPSTREAM"
    exit 1
fi

LOG_STATUS=$(echo "$LOG_LINE" | python3 -c "import sys, json; print(json.load(sys.stdin).get('status',''))")
if [ "$LOG_STATUS" != "200" ]; then
    echo "❌ Access log status mismatch: $LOG_STATUS"
    exit 1
fi

METRICS_OUT="$TEST_TMP/metrics.txt"
curl -s "http://127.0.0.1:$PORT_METRICS" > "$METRICS_OUT"
if ! grep -q 'pavis_http_requests_total{method="GET",route="/consistent",status="200",upstream="backend-consistent"} 1' "$METRICS_OUT"; then
    echo "❌ Metrics missing for /consistent"
    grep "pavis_http_requests_total" "$METRICS_OUT"
    exit 1
fi
if ! grep -q 'pavis_http_requests_total{method="GET",route="/echo",status="200",upstream="backend-consistent"} 2' "$METRICS_OUT"; then
    echo "❌ Metrics missing or incorrect count for /echo"
    grep "pavis_http_requests_total" "$METRICS_OUT"
    exit 1
fi
if ! grep -q 'pavis_upstream_requests_total{upstream="backend-consistent",status="200"} 3' "$METRICS_OUT"; then
    echo "❌ Metrics missing or incorrect count for upstream_requests_total"
    grep "pavis_upstream_requests_total" "$METRICS_OUT"
    exit 1
fi

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

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

sleep 2

pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo"

METRICS_RELOAD="$TEST_TMP/metrics_reload.txt"
curl -s "http://127.0.0.1:$PORT_METRICS" > "$METRICS_RELOAD"
if ! grep -q 'pavis_http_requests_total{method="GET",route="/echo",status="200",upstream="backend-consistent"} 3' "$METRICS_RELOAD"; then
    echo "❌ Metrics reset or lost during hot-reload"
    grep "pavis_http_requests_total" "$METRICS_RELOAD"
    exit 1
fi

echo "✅ obs_consistency passed"
