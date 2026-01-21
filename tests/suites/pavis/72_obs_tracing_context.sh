#!/bin/bash
set -e

# Case: obs_03_tracing_context
# Category: Observability
# Invariants: D (Zero-Option)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "obs_03"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
UPSTREAM_PORT=${UPSTREAM_HTTP_PORT_V1}

# 1. Config with Tracing Enabled
cat <<EOF > "$TEST_TMP/config.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  metrics: "127.0.0.1:$(get_free_port)"
  tracing:
    provider: "otlp"
    endpoint: "http://127.0.0.1:4317"
    sampling: 100
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: $UPSTREAM_PORT
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/echo" }
        destinations:
          - upstream: "backend"
            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 2. Start Pavis
run_pavis "$TEST_TMP/config.pvs" ""

# 3. Wait for boot
wait_for_port "$PORT_PAVIS" 5
# Give some time for async tracing initialization
sleep 2

# 4. Generate Traffic
# We expect Pavis to generate a traceparent header for upstream requests.
RESPONSE_V1=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
if ! echo "$RESPONSE_V1" | grep -q '"traceparent"'; then
    echo "❌ traceparent header missing when tracing is enabled"
    exit 1
fi

# 5. Restart with tracing disabled (sampling 0)
stop_sut "pavis"

cat <<EOF > "$TEST_TMP/config_v2.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:$PORT_PAVIS"
telemetry:
  tracing:
    provider: "otlp"
    endpoint: "http://127.0.0.1:4317"
    sampling: 0
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: $UPSTREAM_PORT
routes:
  - host: "*"
    paths:
      - matcher:
          path: !prefix { path: "/echo" }
        destinations:
          - upstream: "backend"
            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
run_pavis "$TEST_TMP/config_v2.pvs" ""
wait_for_port "$PORT_PAVIS" 5
sleep 1

# 6. Verify Tracing Stopped
RESPONSE_V2=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
if ! echo "$RESPONSE_V2" | grep -q '"traceparent"'; then
    echo "✅ obs_03_tracing_context passed"
else
    echo "❌ traceparent header present when tracing is disabled"
    exit 1
fi
