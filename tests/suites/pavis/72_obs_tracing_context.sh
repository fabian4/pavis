#!/bin/bash
set -e

# Case: obs_03_tracing_context
# Category: Observability
# Invariants: D (Zero-Option)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "obs_03"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
UPSTREAM_PORT=8081

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
# Give some time for async tracing initialization
sleep 2

# 4. Generate Traffic
# We expect Pavis to generate a trace_id and propagate it
_=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")

# 5. Hot Reload: Disable Tracing
PORT_RELAY=$(get_free_port)
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V2: Sampling 0
cat <<EOF > "$TEST_TMP/config_v2.yaml"
listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
telemetry:
  tracing: { provider: "otlp", endpoint: "http://127.0.0.1:4317", sampling: 0 }
upstreams: [{ name: "backend", endpoints: [{ ip: "127.0.0.1", port: $UPSTREAM_PORT }] }]
routes:
  - host: "*"
    paths: [{ matcher: !prefix { path: "/echo" }, destinations: [{ upstream: "backend", weight: 1 }] }]
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Give some time for reload
sleep 2

# 6. Verify Tracing Stopped
RESPONSE_V2=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")

if echo "$RESPONSE_V2" | grep -q '"traceparent"'; then
    # In some implementations, it might still propagate if the INCOMING request had it.
    # But here Pavis is the one generating it.
    echo "❌ Tracing still active after sampling set to 0"
    exit 1
fi

echo "✅ obs_03_tracing_context passed"
