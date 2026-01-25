#!/bin/bash
set -e

# Case: 80_observability_metrics_contract
# Category: Observability
# Invariants: D (Zero-Option)
# Description: Metrics-only contract checks with delta-based assertions.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "80_observability_metrics_contract"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

echo "STEP: ports"
PORT_PAVIS=$(get_free_port)
PORT_METRICS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_ADMIN=$(get_free_port)
ACCESS_LOG_PATH="$TEST_TMP/access.log"
UPSTREAM_PORT=${UPSTREAM_HTTP_PORT_V1}

# Deterministic access-log drop conditions (test-only)
export PAVIS_ACCESS_LOG_CHANNEL_CAPACITY=1
export PAVIS_ACCESS_LOG_WRITE_THROTTLE_MS=200

echo "STEP: relay"
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

echo "STEP: config v1"
cat <<EOF_CFG > "$TEST_TMP/config_v1.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
  access_log: "$ACCESS_LOG_PATH"
upstreams:
  - name: "backend-consistent"
    endpoints:
      - ip: "127.0.0.1"
        port: $UPSTREAM_PORT
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/echo" }
        destinations: [{ upstream: "backend-consistent", weight: 1 }]
admin:
  enabled: true
  address: "127.0.0.1:$PORT_ADMIN"
EOF_CFG

gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

echo "STEP: config v2"
cat <<EOF_CFG > "$TEST_TMP/config_v2.yaml"
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
      - matcher:
          path: !prefix { path: "/echo" }
        destinations: [{ upstream: "backend-consistent", weight: 1 }]
admin:
  enabled: true
  address: "127.0.0.1:$PORT_ADMIN"
EOF_CFG

gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

echo "STEP: publish v1"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"

echo "STEP: run pavis"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

echo "STEP: wait ready"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5
wait_for_url "http://127.0.0.1:$PORT_METRICS" 5
wait_for_url "http://127.0.0.1:$PORT_ADMIN/health" 5

metrics_value() {
    local pattern="$1"
    pavis_curl_body "http://127.0.0.1:$PORT_METRICS" \
        | tr -d '\r' \
        | grep -E "$pattern" \
        | head -n 1 \
        | awk '{print $2}' || true
}

metrics_value_or_zero() {
    local pattern="$1"
    local value
    value=$(metrics_value "$pattern")
    if [ -z "$value" ]; then
        echo "0"
    else
        echo "$value"
    fi
}

wait_for_metrics_delta() {
    local pattern="$1"
    local before="$2"
    local timeout="${3:-10}"
    local retries=$((timeout * 4))
    local backoff=0.25

    for _ in $(seq 1 $retries); do
        local current
        current=$(metrics_value "$pattern" || echo "")
        if [ -n "$current" ]; then
            if awk -v b="$before" -v c="$current" 'BEGIN {exit !(c - b >= 1)}'; then
                return 0
            fi
        fi
        sleep "$backoff"
    done

    echo "❌ Expected delta >= 1 for pattern '$pattern'"
    curl -s "http://127.0.0.1:$PORT_METRICS"
    return 1
}

get_admin_config_version() {
    pavis_curl_body "http://127.0.0.1:$PORT_ADMIN/stats" | json_get_number "config_version"
}

# Baseline metrics
echo "STEP: baseline metrics"
BASE_RELOAD_COUNT=$(metrics_value_or_zero '^pavis_runtime_reload_count_total')
BASE_ACCESS_LOG_DROPPED=$(metrics_value_or_zero '^pavis_telemetry_access_log_dropped_total')
BASE_SPANS_CREATED=$(metrics_value_or_zero '^pavis_telemetry_tracing_spans_created_total')
BASE_EXPORT_ERRORS=$(metrics_value_or_zero '^pavis_telemetry_tracing_export_errors_total')

# Tracing disabled: spans_created must not increase
echo "STEP: tracing disabled check"
pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo"
SPANS_AFTER_DISABLED=$(metrics_value '^pavis_telemetry_tracing_spans_created_total')
if ! awk -v b="$BASE_SPANS_CREATED" -v c="${SPANS_AFTER_DISABLED:-0}" 'BEGIN {exit !(c - b == 0)}'; then
    echo "❌ spans_created changed while tracing disabled"
    exit 1
fi

# Force access-log drops via backpressure
echo "STEP: access log drop load"
REQUESTS=1000
CONCURRENCY=50
seq 1 "$REQUESTS" | xargs -P "$CONCURRENCY" -n 1 bash -c \
    'curl -s --connect-timeout 1 --max-time 2 -o /dev/null "http://127.0.0.1:'"$PORT_PAVIS"'/echo" >/dev/null 2>&1 || true'

echo "STEP: assert access log drop"
wait_for_metrics_delta '^pavis_telemetry_access_log_dropped_total' "$BASE_ACCESS_LOG_DROPPED" 20

# Publish v2 and wait for /stats config_version to change
echo "STEP: publish v2"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

echo "STEP: wait config version"
MAX_RETRIES=20
attempt=0
for attempt in $(seq 1 $MAX_RETRIES); do
    CURRENT_VERSION=$(get_admin_config_version || echo "")
    if [ "$CURRENT_VERSION" = "2" ]; then
        break
    fi
    sleep 0.5
done
assert_retry_succeeded "$attempt" "$MAX_RETRIES"

echo "STEP: reload metric"
wait_for_metrics_delta '^pavis_runtime_reload_count_total' "$BASE_RELOAD_COUNT" 10

# Tracing enabled: spans_created and export_errors must increase
echo "STEP: tracing enabled load"
for _ in $(seq 1 5); do
    pavis_curl_body -o /dev/null "http://127.0.0.1:$PORT_PAVIS/echo"
done

echo "STEP: assert tracing metrics"
wait_for_metrics_delta '^pavis_telemetry_tracing_spans_created_total' "$BASE_SPANS_CREATED" 10
wait_for_metrics_delta '^pavis_telemetry_tracing_export_errors_total' "$BASE_EXPORT_ERRORS" 10

# Gauge presence: config version label should appear at least once
echo "STEP: assert config gauge"
METRICS_OUT=$(curl -s "http://127.0.0.1:$PORT_METRICS" | tr -d '\r')
if ! echo "$METRICS_OUT" | grep -q 'pavis_runtime_config_version{version="2"}'; then
    echo "❌ Expected config version label not found"
    echo "$METRICS_OUT" | grep "pavis_runtime_config_version" || true
    exit 1
fi

# Route metric presence (no absolute counts)
echo "STEP: assert route metrics"
if ! echo "$METRICS_OUT" | grep -q 'pavis_http_requests_total{.*route="/echo"'; then
    echo "❌ Missing request metrics for /echo"
    echo "$METRICS_OUT" | grep "pavis_http_requests_total" || true
    exit 1
fi

echo "✅ observability_metrics_contract passed"
