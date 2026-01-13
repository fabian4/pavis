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
RESPONSE=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")

# 5. Assertions
# The upstream echoes headers. We check for 'traceparent'.
# W3C Trace Context: traceparent header
if ! echo "$RESPONSE" | grep -q '"traceparent"'; then
    # Fallback check for uber-trace-id (Jaeger/Zipkin legacy) just in case
    if ! echo "$RESPONSE" | grep -q '"uber-trace-id"'; then
        echo "⚠️ Tracing headers not found in upstream response."
        echo "Headers received by upstream:"
        echo "$RESPONSE" | grep '"headers"' -A 20
        # Fail the test? Or is propagation not strictly promised in Phase 5 check?
        # The plan called it "Distributed Tracing". Without propagation, it is local.
        # I will mark it as failure to highlight the gap.
        echo "❌ Context propagation failed"
        exit 1
    fi
fi

echo "✅ obs_03_tracing_context passed"
