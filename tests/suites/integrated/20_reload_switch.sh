#!/bin/bash
set -e

# Case: reload_01_traffic_shift
# Category: End-to-End Reload
# Invariants: I1, I2, I5

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "reload_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

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

# 2. Config V1
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8081
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

# Publish V1
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/config_v1.pvs" > /dev/null

# Start Pavis (using V1 as initial LKG)
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert V1
response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
  -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
  -H "X-Pavis-Test-Case: ${CASE_NAME}")
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['instance_id'])")
if [ "$instance" != "backend-v1" ]; then
    echo "❌ Expected backend-v1, got $instance"
    exit 1
fi

# Capture PID
if [ "$TEST_MODE" == "binary" ]; then
    PID_INITIAL=$(cat "$TEST_TMP/pids/pavis.pid")
fi

# 3. Publish V2
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8082
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/config_v2.pvs" > /dev/null

# 4. Wait for Switch
MAX_RETRIES=20
SWITCHED=0
for i in $(seq 1 $MAX_RETRIES); do
    response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
      -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
      -H "X-Pavis-Test-Case: ${CASE_NAME}")
    instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id', ''))")
    
    if [ "$instance" == "backend-v2" ]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Traffic did not switch to backend-v2"
    exit 1
fi

# 5. Assert PID
if [ "$TEST_MODE" == "binary" ]; then
    PID_FINAL=$(cat "$TEST_TMP/pids/pavis.pid")
    if [ "$PID_INITIAL" != "$PID_FINAL" ]; then
        echo "❌ PID changed! Pavis restarted."
        exit 1
    fi
fi

echo "✅ reload_01_traffic_shift passed"