#!/bin/bash
set -e

# Case: traffic_95_retry_budget
# Category: Resilience - P2 Feature Verification
# Invariant: Global timeout prevents infinite/excessive retries
#
# Config:
#   request_timeout = 1000ms
#   max_attempts = 5
#   retry_on = ["status_code"] (503)
#   backoff = fixed, base_ms=300
#
# Upstream behavior:
#   Delay 100ms, then return 503.
#
# Timeline:
#   Attempt 1: 0ms -> 100ms (Fail). Remaining: 900ms. Backoff: 300ms.
#   Attempt 2: 400ms -> 500ms (Fail). Remaining: 500ms. Backoff: 300ms.
#   Attempt 3: 800ms -> 900ms (Fail). Remaining: 100ms. Backoff: 300ms.
#   Next attempt requires backoff 300ms, but budget is 100ms.
#   Should fail with 504 Gateway Timeout.
#
# Verdict: Client receives 504.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "traffic_95"
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
telemetry: {}
upstreams:
  - name: "backend"
    endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_HTTP_PORT_V1 }]
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/" }
        timeout: "1000ms"
        retry:
          attempts: 5
          retry_on: ["status_code"]
          retryable_status_codes: [503]
          backoff:
             strategy: "fixed"
             base_ms: 300
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

echo "== Phase A: Global Budget Exhaustion =="

FLAKY_ID="case-95-$(date +%s)"
# 200ms delay:
# Att 1: 200ms + 300ms backoff = 500ms
# Att 2: 200ms + 300ms backoff = 1000ms
# Att 3 starts at 1000ms -> Timeout
URL="http://127.0.0.1:$PORT_PAVIS/flaky?id=$FLAKY_ID&code=503&times=5&delay_ms=200"

echo "Sending request expecting 504 Gateway Timeout..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$URL")

if [ "$STATUS" != "504" ]; then
    echo "❌ Expected status 504 (timeout), got $STATUS"
    exit 1
fi

echo "✅ traffic_95_retry_budget passed"