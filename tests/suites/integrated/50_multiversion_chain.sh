#!/bin/bash
set -e

# Case: multiversion_01_chain_apply
# Category: End-to-End Reload
# Invariants: I2 (Monotonic), I5 (No Regression)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "multiversion_50"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

# 1. Start Relay
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

# V1
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v1"]]
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

# V2
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v2"]]
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

# V3
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
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v3"]]
	        destinations:
	          - upstream: "backend-v3"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"

# V4
cat <<-EOF > "$TEST_TMP/config_v4.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend-v4"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v4"]]
	        destinations:
	          - upstream: "backend-v4"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v4.yaml" "$TEST_TMP/config_v4.pvs"

# Publish V1 and start runtime
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/config_v1.pvs" > /dev/null

cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

METRICS_URL="http://127.0.0.1:$PORT_METRICS"
INITIAL_VERSION=$(wait_for_runtime_config_version "$METRICS_URL" 10 || true)
if [ "$INITIAL_VERSION" != "1" ]; then
    echo "❌ Expected initial runtime version 1, got '$INITIAL_VERSION'"
    exit 1
fi

# Publish V2 -> V3 -> V4 rapidly
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/config_v2.pvs" > /dev/null
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 3" \
    --data-binary "@$TEST_TMP/config_v3.pvs" > /dev/null
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 4" \
    --data-binary "@$TEST_TMP/config_v4.pvs" > /dev/null

wait_for_version() {
    local expected="$1"
    local timeout="${2:-20}"
    local retries=$((timeout * 4))
    local backoff=0.25

    for _ in $(seq 1 $retries); do
        version=$(get_runtime_config_version "$METRICS_URL") || true
        if [ "$version" = "$expected" ]; then
            return 0
        fi
        sleep "$backoff"
    done
    return 1
}

assert_version_header() {
    local expected="$1"
    local headers="$TEST_TMP/resp_v${expected}.headers"
    pavis_curl_headers "$headers" "http://127.0.0.1:$PORT_PAVIS/echo"
    if ! grep -qi "^X-Pavis-Version: v${expected}" "$headers"; then
        echo "❌ Missing X-Pavis-Version: v${expected} header"
        exit 1
    fi
}

for expected in 2 3 4; do
    if ! wait_for_version "$expected" 20; then
        echo "❌ Runtime did not apply version $expected"
        exit 1
    fi
    assert_version_header "$expected"
done

if ! check_sut_alive "pavis"; then
    echo "❌ Pavis died during multi-version chain"
    exit 1
fi

echo "✅ multiversion_01_chain_apply passed"
