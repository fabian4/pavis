#!/bin/bash
set -e

# Case: lifecycle_03_lkg_corruption
# Category: Failure & LKG
# Invariants: B (LKG)

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "lifecycle_03"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Mock Relay
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# 2. Prepare Config V1
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

# 3. Publish V1 and Start
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# 4. Assert V1
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
echo "$response" | assert_json_has_key "instance_id"
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['instance_id'])")
if [ "$instance" != "backend-v1" ]; then
    echo "❌ Expected backend-v1, got $instance"
    exit 1
fi

# 5. Publish Corruption
echo "THIS_IS_NOT_A_VALID_PVS_FILE_RANDOM_BYTES" > "$TEST_TMP/corrupt.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/corrupt.pvs"

# 6. Wait for potential poll cycle
sleep 2

# 7. Assert LKG (Still V1)
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id', ''))")

if [ "$instance" != "backend-v1" ]; then
    echo "❌ LKG failed. Expected backend-v1, got '$instance'"
    exit 1
fi

# 8. Recoverability Proof: Publish Valid V3
cat <<-EOF > "$TEST_TMP/config_v3.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v3"
	    endpoints: [{ ip: "127.0.0.1", port: 8082 }]
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations: [{ upstream: "backend-v3", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_v3.yaml" "$TEST_TMP/config_v3.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v3.pvs"

# 9. Wait for V3 switch
MAX_RETRIES=20
SWITCHED=0
for _ in $(seq 1 $MAX_RETRIES); do
    response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
    if [[ "$response" == *"backend-v2"* ]]; then # backend-v2 is mock-upstream id for port 8082
        SWITCHED=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Recovery failed: Runtime did not switch to V3 after corruption"
    exit 1
fi

# 10. Assert Process Alive
if ! check_sut_alive "pavis"; then
    echo "❌ Pavis died after receiving corrupt config!"
    exit 1
fi

echo "✅ lifecycle_03_lkg_corruption passed"
