#!/bin/bash
set -e

# Case: 30_lkg_artifact
# Category: Failure & LKG
# Invariants: I3 (Artifact Opaqueness), I4 (System LKG)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "30_lkg_artifact"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	artifact:
	  lkg_path: "$TEST_TMP/lkg.pvs"
EOF
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/config.pvs" > /dev/null

cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

METRICS_URL="http://127.0.0.1:$PORT_METRICS"
BASELINE_RUNTIME_VERSION=$(wait_for_runtime_config_version "$METRICS_URL" 10 || true)
if [ -z "$BASELINE_RUNTIME_VERSION" ]; then
    echo "❌ Missing runtime config version metric"
    exit 1
fi

# Assert V1
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# Publish Corrupt Data
# The relay MUST reject corrupt artifacts (magic/checksum validation).
echo "CORRUPT" > "$TEST_TMP/corrupt.pvs"
RESP=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/corrupt.pvs")

echo "Publish response: $RESP"

RELAY_VERSION=$(get_relay_config_version "http://127.0.0.1:$PORT_RELAY")
RUNTIME_VERSION=$(get_runtime_config_version "$METRICS_URL")
if [ -z "$RELAY_VERSION" ] || [ -z "$RUNTIME_VERSION" ]; then
    echo "❌ Missing relay/runtime version after corrupt publish"
    exit 1
fi
if [ "$RESP" -lt 100 ]; then
    echo "❌ Publish request failed (no HTTP response)"
    exit 1
fi

if [ "$RESP" -lt 400 ]; then
    echo "❌ Relay accepted corrupt artifact (expected 4xx rejection)"
    exit 1
fi
if [ "$RELAY_VERSION" -ne "$RUNTIME_VERSION" ]; then
    echo "❌ Relay advanced despite rejecting corrupt publish"
    exit 1
fi

# Assert Traffic Continues

assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# 5. Recovery Proof: Publish Valid V3

cat <<-EOF > "$TEST_TMP/config_v3.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v3"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v3"
	            weight: 1
EOF

gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"

curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" -H "x-pavis-version: 3" --data-binary "@$TEST_TMP/config_v3.pvs" > /dev/null

# 6. Assert Switch to V3

MAX_RETRIES=20
SWITCHED=0
attempt=0
for attempt in $(seq 1 $MAX_RETRIES); do
    if pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo" | grep -q "backend-v2"; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

assert_retry_succeeded "$attempt" "$MAX_RETRIES"

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Integrated recovery failed"
    exit 1
fi

echo "✅ 30_lkg_artifact passed"
