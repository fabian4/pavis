#!/bin/bash
set -e

# Case: traffic_03_headers
# Category: Traffic Management
# Description: Verifies Request and Response header manipulation.

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "traffic_03"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Mock Relay
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# 2. Config: Header manipulation
cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8081
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/echo" }
	        request_headers:
	          set_headers:
	            - ["X-Request-Set", "pavis-set"]
	          append_headers:
	            - ["X-Request-Append", "pavis-appended"]
	          add_headers:
	            - ["X-Request-Add", "pavis-added"]
	          remove_headers:
	            - "X-To-Remove"
	        response_headers:
	          set_headers:
	            - ["X-Response-Set", "pavis-resp-set"]
	          remove_headers:
	            - "X-Internal-Header"
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 3. Start Pavis
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# 4. Assert Request Headers (via /echo)
echo "--- Testing Request Headers ---"
# We send X-To-Remove and X-Request-Append: original
response=$(curl -s -H "X-To-Remove: should-be-gone" -H "X-Request-Append: original" "http://127.0.0.1:$PORT_PAVIS/echo")

# Check X-Request-Set
val=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['headers'].get('x-request-set', [''])[0])")
assert_eq "pavis-set" "$val" "X-Request-Set should be set"

# Check X-Request-Append
val=$(echo "$response" | python3 -c "import sys, json; h=json.load(sys.stdin)['headers']; print(', '.join(h.get('x-request-append', [])))")
# Pavis joinable headers are collapsed with ", " by the proxy itself, 
# so upstream sees ONE value "original, pavis-appended"
assert_eq "original, pavis-appended" "$val" "X-Request-Append should be appended"

# Check X-Request-Add
val=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['headers'].get('x-request-add', [''])[0])")
assert_eq "pavis-added" "$val" "X-Request-Add should be added"

# Check X-To-Remove
val=$(echo "$response" | python3 -c "import sys, json; print('PRESENT' if 'x-to-remove' in json.load(sys.stdin)['headers'] else 'ABSENT')")
assert_eq "ABSENT" "$val" "X-To-Remove should be removed"

# 5. Assert Response Headers
echo "--- Testing Response Headers ---"
resp_headers=$(curl -sI "http://127.0.0.1:$PORT_PAVIS/echo")

# Check X-Response-Set
if ! echo "$resp_headers" | grep -qi "X-Response-Set: pavis-resp-set"; then
    echo "❌ X-Response-Set header missing in response"
    exit 1
fi

# Check X-Proxy-By (automatic)
if ! echo "$resp_headers" | grep -qi "X-Proxy-By: Pavis"; then
    echo "❌ X-Proxy-By header missing in response"
    exit 1
fi

echo "✅ traffic_03_headers passed"
