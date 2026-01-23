#!/bin/bash
set -e

# Case: 21_reload_storm
# Category: Reload Semantics
# Invariants: A (No-Drop), C (Atomic Switch), State Leaks (RSS check)
#
# This test verifies traffic continuity and atomic switching during a rapid 
# sequence of configuration updates ("reload storm"). 
# It also verifies that memory usage (RSS) does not leak significantly.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "21_reload_storm"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
PORT_METRICS=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

write_config() {
    local version="$1"
    local upstream="$2"
    cat <<-EOF
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
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V2}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v${version}"]]
	        destinations:
	          - upstream: "${upstream}"
	            weight: 1
EOF
}

# --- Step 0: Baseline (v1) ---
write_config 1 "backend-v1" > "$TEST_TMP/config_v1.yaml"
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

# Capture Initial RSS
PAVIS_PID=$(get_sut_host_pid "pavis")
RSS_START=$(ps -o rss= -p "$PAVIS_PID" | tr -d ' ')
echo "Initial RSS: ${RSS_START} KB"

# --- Step 1: Rapid Reloads (v2..v10) ---
TOTAL_TRAFFIC=1000
(
    for i in $(seq 1 $TOTAL_TRAFFIC); do
        headers="$TEST_TMP/traffic_${i}.headers"
        body="$TEST_TMP/traffic_${i}.body"
        if ! curl -sS -D "$headers" -o "$body" "http://127.0.0.1:$PORT_PAVIS/echo"; then
            echo "curl failed" > "$TEST_TMP/traffic_${i}.fail"
            continue
        fi
        version=$(grep -i "^X-Pavis-Version:" "$headers" | awk '{print $2}' | tr -d '\r')
        if [ -z "$version" ]; then
            echo "missing version header" > "$TEST_TMP/traffic_${i}.fail"
            continue
        fi
        if ! instance=$(json_get_string "instance_id" < "$body" 2>/dev/null); then
            echo "failed to parse instance_id from: $(cat "$body")" > "$TEST_TMP/traffic_${i}.fail"
            continue
        fi
        echo "${version},${instance}" > "$TEST_TMP/traffic_${i}.info"
        sleep 0.01
    done
) &
TRAFFIC_PID=$!

echo "Performing 9 rapid reloads..."
for v in $(seq 2 10); do
    upstream="backend-v1"
    [ $((v % 2)) -eq 0 ] && upstream="backend-v2"
    
    write_config "$v" "$upstream" > "$TEST_TMP/config_v${v}.yaml"
    gen_pvs "$TEST_TMP/config_v${v}.yaml" "$TEST_TMP/config_v${v}.pvs"
    publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v${v}.pvs"

    if ! wait_for_runtime_config_version "http://127.0.0.1:$PORT_METRICS/metrics" "$v" 10; then
        fail "Reload storm did not apply version v${v}"
    fi
done

wait $TRAFFIC_PID

# --- Step 2: Final Convergence (v10) ---
echo "Waiting for final convergence to v10..."
if ! wait_for_runtime_config_version "http://127.0.0.1:$PORT_METRICS/metrics" "10" 15; then
    fail "Reload storm did not converge to final version 10"
fi

# --- Step 3: Analyze Traffic ---
shopt -s nullglob
fail_files=($"$TEST_TMP"/traffic_*.fail)
shopt -u nullglob
FAIL_COUNT=${#fail_files[@]}
if [ "$FAIL_COUNT" -gt 0 ]; then
    echo "Sample failures:"
    head -n 5 "$TEST_TMP"/traffic_*.fail
    fail "Traffic dropped or invalid during reload storm: $FAIL_COUNT requests failed"
fi

MAX_SEEN=0
for i in $(seq 1 $TOTAL_TRAFFIC); do
    info="$TEST_TMP/traffic_${i}.info"
    [ -f "$info" ] || continue
    
    entry=$(cat "$info")
    version=$(echo "$entry" | cut -d',' -f1)
    instance=$(echo "$entry" | cut -d',' -f2)
    
    version_num=$(echo "$version" | tr -d 'v')
    
    # Verify version/instance consistency
    expected_instance="backend-v1"
    [ $((version_num % 2)) -eq 0 ] && expected_instance="backend-v2"
    if [ "$instance" != "$expected_instance" ]; then
        fail "Consistency violation at request $i: version $version served by $instance (expected $expected_instance)"
    fi
    
    # Verify Monotonicity
    if [ "$version_num" -lt "$MAX_SEEN" ]; then
        fail "Monotonicity violation at request $i: saw v${version_num} after v${MAX_SEEN}"
    fi
    MAX_SEEN=$version_num
done
echo "Monotonicity verified. Highest version seen: v$MAX_SEEN"

# --- Step 4: Memory Assertion ---
echo "Verifying memory stability..."
# Give it a moment to settle/GC
sleep 2
RSS_END=$(ps -o rss= -p "$PAVIS_PID" | tr -d ' ')
echo "Final RSS: ${RSS_END} KB"

DELTA=$((RSS_END - RSS_START))
# Allow up to 50% growth to account for transient buffers and caches.
MAX_DELTA=$((RSS_START / 2))
if [ "$DELTA" -gt "$MAX_DELTA" ]; then
    fail "Significant memory increase detected after reload storm: +${DELTA} KB (>${MAX_DELTA} KB)"
fi

if ! check_sut_alive "pavis"; then
    fail "Pavis died during reload storm"
fi

echo "✅ reload_storm passed"
