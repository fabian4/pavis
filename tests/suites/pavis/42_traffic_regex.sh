#!/bin/bash
set -e

# Case: traffic_02_matcher_regex
# Category: Traffic Management
# Description: Verifies Regex route matching.

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "traffic_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Mock Relay
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# 2. Config: Regex matching
cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8081
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8082
	routes:
	  - host: "*"
	    paths:
	      - matcher: !regex { path: '^/echo$' }
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 3. Start Pavis
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# 4. Assert Regex Match (v1)
echo "--- Testing Regex Match ---"
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['instance_id'])")
assert_eq "backend-v1" "$instance" "Should match regex route for /echo"

# 5. Assert Regex Miss (v2 via prefix /)
echo "--- Testing Regex Miss (Fallback to prefix) ---"
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/id")
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin).get('id', ''))")
assert_eq "backend-v2" "$instance" "Should NOT match regex route, falling back to prefix"

echo "✅ traffic_02_matcher_regex passed"
