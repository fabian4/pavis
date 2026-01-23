#!/bin/bash
set -e

# Case: 66_resilience_retry_idempotency
# Category: Resilience - P2 Feature Verification
# Invariant: POST requests are NOT retried by default (unless retry_non_idempotent=true)
#
# Config: max_attempts=3, retryable_status_codes=[503], retry_non_idempotent=false
# Test: Send POST to flaky endpoint (503)
# Verdict: Client receives 503 (no retry)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "66_resilience_retry_idempotency"
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
        retry:
          attempts: 3
          retry_on: ["status_code"]
          retryable_status_codes: [503]
          retry_non_idempotent: false
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

echo "== Phase A: POST Idempotency (Expect 503 immediately) =="

FLAKY_ID="case-94-$(date +%s)"
URL="http://127.0.0.1:$PORT_PAVIS/flaky?id=$FLAKY_ID&code=503&times=1"

echo "Sending POST request to flaky endpoint..."
STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$URL")

if [ "$STATUS" != "503" ]; then
    echo "❌ Expected status 503 (no retry), got $STATUS"
    exit 1
fi

echo "== Phase B: POST with retry_non_idempotent=true =="

cat <<-EOF > "$TEST_TMP/config_v2.yaml"
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
        retry:
          attempts: 3
          retry_on: ["status_code"]
          retryable_status_codes: [503]
          retry_non_idempotent: true
        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Wait for config reload
wait_for_reload 10

FLAKY_ID_V2="case-94-v2-$(date +%s)"
URL_V2="http://127.0.0.1:$PORT_PAVIS/flaky?id=$FLAKY_ID_V2&code=503&times=1"

echo "Sending POST request with retry_non_idempotent=true..."
STATUS_V2=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$URL_V2")

if [ "$STATUS_V2" != "200" ]; then
    echo "❌ Expected status 200 (retry successful), got $STATUS_V2"
    exit 1
fi

echo "✅ traffic_66_resilience_retry_idempotency passed"
