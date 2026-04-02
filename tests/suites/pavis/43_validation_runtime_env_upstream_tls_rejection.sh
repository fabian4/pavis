#!/bin/bash
set -e

# Case: 43_validation_runtime_env_upstream_tls_rejection
# Category: Failure & LKG
# Invariants: B (LKG), Runtime env validation, A (No-Drop)
#
# This test verifies that a runtime-safe reload can still fail runtime env
# validation when it introduces unreadable upstream TLS files, and that the
# system keeps serving the LKG.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "43_validation_runtime_env_upstream_tls_rejection"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

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
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"

cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# --- Step 0b: Start Traffic Continuity Loop ---
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

# --- Step 1: Publish Invalid Runtime-Safe Config (v2) ---
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "backend"
	    tls:
	      enabled: true
	      verify_cert: true
	      verify_hostname: true
	      sni_mode: name
	      sni: "localhost"
	      ca_bundle_path: "$TEST_TMP/missing_upstream_ca.pem"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# --- Step 2: Verify Rejection ---
echo "Waiting for runtime validation failure metric..."
if ! assert_metric_at_least 'pavis_config_validation_total.*result="fail".*reason="runtime"' 1 15; then
    fail "Expected runtime validation failure metric"
fi

if ! grep -aq "result=.fail. reason=.runtime." "$TEST_TMP/logs/pavis.log"; then
    echo "WARN: Rejection metric present but log entry not found via grep"
fi

# --- Step 3: Serving State Assertion ---
echo "Verifying system remains in LKG state (v1)..."
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

VERSION=$(get_runtime_config_version "http://127.0.0.1:$PORT_METRICS/metrics")
if [ "$VERSION" != "1" ]; then
    fail "Runtime configuration version mismatch: expected 1, got $VERSION"
fi

if ! check_sut_alive "pavis"; then
    fail "Pavis died during runtime env validation"
fi

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
    fail "Traffic was interrupted during runtime env rejection"
fi

echo "✅ runtime_env_upstream_tls_rejection passed"
