#!/bin/bash
set -e

# Case: lifecycle_02_hot_reload_basic
# Category: Reload Semantics
# Invariants: A (No-Drop), C (Atomic Switch)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "lifecycle_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Mock Relay
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# 2. Prepare Configs
# V1: Routes to backend-v1 (8081)
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
	      - matcher: !prefix
	          path: "/"
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

# V2: Routes to backend-v2 (8082)
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
	      - matcher: !prefix
	          path: "/"
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

# 3. Publish V1 to Relay
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"

# 4. Start Pavis (Connected to Relay)
# We use V1 as the initial LKG file.
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"

wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# 5. Assert V1 Traffic
response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
  -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
  -H "X-Pavis-Test-Case: ${CASE_NAME}")
echo "$response" | assert_json_has_key "instance_id"
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['instance_id'])")
if [ "$instance" != "backend-v1" ]; then
    echo "❌ Expected backend-v1 initially, got $instance"
    exit 1
fi

# Capture PID to ensure no restart
if [ "$TEST_MODE" == "binary" ]; then
    PID_INITIAL=$(cat "$TEST_TMP/pids/pavis.pid")
fi

# 6. Publish V2 to Relay (Hot Reload)
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# 7. Wait for switch-over
# The runtime polls every ~1s (with backoff). We poll the endpoint until it switches.
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
    echo "❌ Traffic did not switch to backend-v2 after reload"
    exit 1
fi

# 8. Assert PID Constant
if [ "$TEST_MODE" == "binary" ]; then
    PID_FINAL=$(cat "$TEST_TMP/pids/pavis.pid")
    if [ "$PID_INITIAL" != "$PID_FINAL" ]; then
        echo "❌ PID changed! Pavis restarted."
        exit 1
    fi
    # Ensure process is still running
    if ! kill -0 "$PID_FINAL" 2>/dev/null; then
        echo "❌ Pavis process died!"
        exit 1
    fi
fi

echo "✅ lifecycle_02_hot_reload_basic passed"
