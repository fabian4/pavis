#!/bin/bash
set -e

# Case: 31_lkg_rejection
# Category: Failure & LKG
# Invariants: I4 (System LKG), A2 (Immutable Execution State)
#
# This test verifies that runtime environment validation rejects invalid configs
# before apply and preserves LKG while traffic continues.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "31_lkg_rejection"
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
EOF
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# --- Step 0: Baseline (v1) ---
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
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
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/config_v1.pvs" > /dev/null

cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# --- Step 1: Start Traffic Loop ---
BURST_COUNT=120
(
    for i in $(seq 1 $BURST_COUNT); do
        if ! curl -sS "http://127.0.0.1:$PORT_PAVIS/echo" | grep -q "backend-v1"; then
            echo "saw invalid content or failure" > "$TEST_TMP/traffic_$i.fail"
        fi
        sleep 0.05
    done
) &
TRAFFIC_PID=$!

# --- Step 2: Publish Invalid Config (v2) ---
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	    tls:
	      cert_path: "$TEST_TMP/missing_cert.pem"
	      key_path: "$TEST_TMP/missing_key.pem"
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
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

echo "Publishing invalid config (missing certs)..."
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/config_v2.pvs" > /dev/null

echo "Waiting for runtime validation failure..."
if ! wait_for_metric "pavis_config_validation_total.*result=\"fail\".*reason=\"runtime\"" "> 0" 15 "http://127.0.0.1:$PORT_METRICS/metrics"; then
    if ! wait_for_log "config_validation.*fail.*reason.*runtime" "$TEST_TMP/logs/pavis.log" 5; then
        fail "Runtime did not report env validation failure"
    fi
fi

# --- Step 3: Verify Continuity and LKG ---
wait $TRAFFIC_PID

shopt -s nullglob
fail_files=()
for f in "$TEST_TMP"/traffic_*.fail; do
    fail_files+=("$f")
done
shopt -u nullglob
if [ ${#fail_files[@]} -gt 0 ]; then
    echo "Traffic failures encountered:"
    head -n 5 "$TEST_TMP"/traffic_*.fail
    fail "Traffic interrupted during env rejection"
fi

assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

VERSION=$(get_runtime_config_version "http://127.0.0.1:$PORT_METRICS/metrics")
if [ "$VERSION" != "1" ]; then
    fail "Runtime configuration version changed during rejection (expected 1, got $VERSION)"
fi

if ! check_sut_alive "pavis"; then
    fail "Pavis died during runtime env validation"
fi

echo "✅ 31_lkg_rejection passed"
