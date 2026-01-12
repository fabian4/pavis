#!/bin/bash
set -e

# Case: traffic_04_actions
# Category: Traffic Management
# Description: Verifies Redirect and Direct Response actions.

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "traffic_04"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Mock Relay
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# 2. Config: Redirect and Direct Response
cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams: []
	routes:
	  - host: "*"
	    paths:
	      - matcher: !exact { path: "/redirect-me" }
	        status: 301
	        location: "http://example.com/new-location"
	      - matcher: !exact { path: "/direct-me" }
	        status: 200
	        body: "Custom Static Response Body"
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 3. Start Pavis
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# 4. Assert Redirect
echo "--- Testing Redirect Action ---"
resp_headers=$(curl -sI "http://127.0.0.1:$PORT_PAVIS/redirect-me")
status=$(echo "$resp_headers" | head -n 1 | awk '{print $2}')
assert_eq "301" "$status" "Status should be 301"

location=$(echo "$resp_headers" | grep -i "Location:" | awk '{print $2}' | tr -d '\r')
assert_eq "http://example.com/new-location" "$location" "Location should be redirected"

# 5. Assert Direct Response
echo "--- Testing Direct Response Action ---"
response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/direct-me")
assert_eq "Custom Static Response Body" "$response" "Body should match static content"

resp_headers=$(curl -sI "http://127.0.0.1:$PORT_PAVIS/direct-me")
status=$(echo "$resp_headers" | head -n 1 | awk '{print $2}')
assert_eq "200" "$status" "Status should be 200"

echo "✅ traffic_04_actions passed"
