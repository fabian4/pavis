#!/bin/bash
set -e

# Case: 42_validation_atomicity_partial_corruption
# Category: Reliability
# Invariants: A4 (Atomic Validity)
#
# This test verifies that Pavis rejects configuration artifacts that are partially corrupt
# (valid header/magic, but truncated or invalid payload).
# The runtime must NOT apply a partial configuration; it must reject the update entirely
# and continue serving the previous valid configuration (LKG).

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "42_validation_atomicity_partial_corruption"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# --- Step 0: Baseline artifact (v1) ---
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
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

# Verify serving v1
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
echo "$response" | grep -q "instance_id" || fail "Expected echo response"
instance=$(echo "$response" | json_get_string "instance_id")
if [ "$instance" != "backend-v1" ]; then
    fail "Expected backend-v1, got $instance"
fi

# --- Step 1: Create Corrupt Artifact (v2) ---
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
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

# Truncate the file to keep the header but corrupt the payload
# Header is typically small (e.g. < 256 bytes). We truncate to 50% size.
SIZE=$(wc -c < "$TEST_TMP/config_v2.pvs")
CUT_SIZE=$((SIZE / 2))
head -c "$CUT_SIZE" "$TEST_TMP/config_v2.pvs" > "$TEST_TMP/config_v2_corrupt.pvs"

echo "Publishing corrupt artifact (Size: $CUT_SIZE bytes)..."
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2_corrupt.pvs"

# Give runtime a moment to poll and reject
sleep 2

# Verify runtime is STILL serving v1
echo "Verifying runtime serving state preservation..."
for i in {1..5}; do
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    instance=$(echo "$response" | json_get_string "instance_id")
    if [ "$instance" != "backend-v1" ]; then
        fail "Runtime switched state despite corruption! Got: $instance"
    fi
    sleep 0.2
done

# --- Step 2: Recover with Valid Artifact (v3) ---
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

echo "Publishing valid artifact v3..."
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v3.pvs"

echo "Waiting for runtime to apply v3..."
# Wait for metric version 3
if ! wait_for_runtime_config_version "http://127.0.0.1:$PORT_METRICS/metrics" "3" 30; then
    fail "Runtime failed to apply valid config v3 (metric check timed out)"
fi

# Verify serving v3
MAX_RETRIES=20
SWITCHED=0
for i in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    instance=$(echo "$response" | json_get_string "instance_id")
    if [ "$instance" = "backend-v2" ]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

assert_retry_succeeded "$i" "$MAX_RETRIES"

if [ "$SWITCHED" -eq 0 ]; then
    fail "Runtime did not switch to backend-v2 (v3 config)"
fi

if ! check_sut_alive "pavis"; then
    fail "Pavis died during test"
fi

echo "✅ pavis_42_validation_atomicity_partial_corruption passed"