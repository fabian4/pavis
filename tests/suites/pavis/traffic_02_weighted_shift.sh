#!/bin/bash
set -e

# Case: traffic_02_weighted_shift
# Category: Traffic Behavior Under Reload
# Invariants: A (No-Drop)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "traffic_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V1: 100% v1 (Explicit single destination)
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8081
	  - name: "v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8082
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert V1 (100%)
for i in {1..20}; do
    response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
      -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
      -H "X-Pavis-Test-Case: ${CASE_NAME}")
    if [[ "$response" != *"backend-v1"* ]]; then
        echo "❌ Expected only backend-v1 in V1"
        exit 1
    fi
done

# V2: 50% v1, 50% v2
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8081
	  - name: "v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8082
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "v1"
	            weight: 50
	          - upstream: "v2"
	            weight: 50
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Wait for switch (poll for v2 presence)
MAX_RETRIES=20
SWITCHED=0
for i in $(seq 1 $MAX_RETRIES); do
    response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
      -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
      -H "X-Pavis-Test-Case: ${CASE_NAME}")
    if [[ "$response" == *"backend-v2"* ]]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Traffic did not start shifting to backend-v2"
    exit 1
fi

# Assert ~50/50 Distribution (N=100)
c1=0; c2=0
for i in {1..100}; do
    response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
      -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
      -H "X-Pavis-Test-Case: ${CASE_NAME}")
    if [[ "$response" == *"backend-v1"* ]]; then c1=$((c1+1)); else c2=$((c2+1)); fi
done

echo "Distribution: v1=$c1, v2=$c2"
if [ "$c1" -lt 30 ] || [ "$c2" -lt 30 ]; then
    echo "❌ Uneven distribution detected (expected ~50/50)"
    exit 1
fi

echo "✅ traffic_02_weighted_shift passed"