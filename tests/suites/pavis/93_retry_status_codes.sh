#!/bin/bash
set -e

# Case: traffic_93_retry_status_codes
# Category: Resilience - P2 Feature Verification
# Invariant: Retry success after transient status code failures (503 -> 503 -> 200)
#
# Config: max_attempts=3, retryable_status_codes=[503]
# Test: Send request to flaky endpoint configured to fail twice with 503
# Verdict: Client receives 200 OK after 2 retries

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "traffic_93"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

PORT_METRICS=$(get_free_port)

cat <<-EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_HTTP_PORT_V1 }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        retry:
          attempts: 5
          retry_on: ["status_code", "connect_error", "connect_timeout", "read_timeout"]
          retryable_status_codes: [503]
          backoff: { strategy: "fixed", base_ms: 100 }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_url "http://127.0.0.1:$PORT_METRICS/metrics" 5

echo "== Phase A: Retry Success (503 -> 503 -> 200) =="

# Use a unique ID for flaky counter
FLAKY_ID="case-93-$(date +%s)"

# flaky endpoint: code=503, times=2
# http://127.0.0.1:$PORT_PAVIS/flaky?id=$FLAKY_ID&code=503&times=2
URL="http://127.0.0.1:$PORT_PAVIS/flaky?id=$FLAKY_ID&code=503&times=2"

echo "Requesting flaky endpoint (expect success after 2 retries)..."
RESPONSE=$(curl -s -i "$URL")

STATUS=$(echo "$RESPONSE" | head -n 1 | awk '{print $2}')
if [ "$STATUS" != "200" ]; then
    echo "❌ Expected status 200, got $STATUS"
    echo "Response:"
    echo "$RESPONSE"
    exit 1
fi

# Verify via metrics if possible
echo "Checking metrics for retry count..."
METRICS_URL="http://127.0.0.1:$PORT_METRICS/metrics"
assert_metric_at_least 'pavis_upstream_retries_total\{.*upstream="backend",.*reason="status_code",.*attempt="2".*\}' 1 10 "$METRICS_URL"

echo "== Phase B: Retry Exhaustion (All Attempts Fail) =="

# attempts: 5 in config.
# Configure flaky to fail 10 times -> should exhaust 5 attempts and return 503.
FLAKY_ID_EXHAUST="case-93-exhaust-$(date +%s)"
URL_EXHAUST="http://127.0.0.1:$PORT_PAVIS/flaky?id=$FLAKY_ID_EXHAUST&code=503&times=10"

echo "Requesting flaky endpoint (expect 503 after exhaustion)..."
STATUS_EXHAUST=$(curl -s -o /dev/null -w "%{http_code}" "$URL_EXHAUST")
assert_eq "503" "$STATUS_EXHAUST" "Should return 503 after retry exhaustion"

assert_metric_at_least 'pavis_upstream_retry_outcome_total\{.*outcome="exhausted".*\}' 1 10 "$METRICS_URL"

echo "== Phase C: Non-Retryable Status (404) =="

# 404 is not in retryable_status_codes: [503]
FLAKY_ID_404="case-93-404-$(date +%s)"
URL_404="http://127.0.0.1:$PORT_PAVIS/flaky?id=$FLAKY_ID_404&code=404&times=1"

echo "Requesting flaky endpoint with 404 (expect immediate return)..."
STATUS_404=$(curl -s -o /dev/null -w "%{http_code}" "$URL_404")
assert_eq "404" "$STATUS_404" "Should return 404 immediately (no retry)"

echo "== Phase D: Linear Backoff =="

cat <<-EOF > "$TEST_TMP/config_linear.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_HTTP_PORT_V1 }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/linear" }
        retry:
          attempts: 3
          retry_on: ["status_code"]
          retryable_status_codes: [503]
          backoff: { strategy: "linear", base_ms: 200 }
        destinations: [{ upstream: "backend", weight: 1 }]
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_linear.yaml" "$TEST_TMP/config_linear.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_linear.pvs"
wait_for_reload 10
sleep 2

FLAKY_ID_LIN="case-93-linear-$(date +%s)"
URL_LIN="http://127.0.0.1:$PORT_PAVIS/linear/flaky?id=$FLAKY_ID_LIN&code=503&times=2"

echo "Requesting flaky endpoint with linear backoff (path /linear)..."
# We expect success after 2 retries (3rd attempt).
STATUS_LIN=$(curl -s -o /dev/null -w "%{http_code}" "$URL_LIN")
assert_eq "200" "$STATUS_LIN" "Should return 200 with linear backoff on /linear"

echo "== Phase E: Exponential Backoff =="

cat <<-EOF > "$TEST_TMP/config_exp.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$PORT_METRICS"
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_HTTP_PORT_V1 }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/exp" }
        retry:
          attempts: 3
          retry_on: ["status_code"]
          retryable_status_codes: [503]
          backoff: { strategy: "exponential", base_ms: 100, max_ms: 1000 }
        destinations: [{ upstream: "backend", weight: 1 }]
      - matcher:
          path: !prefix { path: "/" }
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_exp.yaml" "$TEST_TMP/config_exp.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_exp.pvs"
wait_for_reload 10
sleep 2

FLAKY_ID_EXP="case-93-exp-$(date +%s)"
URL_EXP="http://127.0.0.1:$PORT_PAVIS/exp/flaky?id=$FLAKY_ID_EXP&code=503&times=2"

echo "Requesting flaky endpoint with exponential backoff (path /exp)..."
STATUS_EXP=$(curl -s -o /dev/null -w "%{http_code}" "$URL_EXP")
assert_eq "200" "$STATUS_EXP" "Should return 200 with exponential backoff on /exp"

echo "✅ traffic_93_retry_status_codes passed"
