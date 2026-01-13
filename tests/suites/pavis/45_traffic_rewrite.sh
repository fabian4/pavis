#!/bin/bash
set -e

# Case: traffic_05_rewrite
# Category: Traffic Management
# Description: Verifies Path and Host rewriting.

# shellcheck source=tests/lib/env.sh
source "$(dirname "$0")/../../lib/env.sh"
# shellcheck source=tests/lib/assert.sh
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "traffic_05"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Mock Relay
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# 2. Config: Rewriting
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
	  - host: "original.com"
	    paths:
	      - matcher: !prefix { path: "/service-a" }
	        rewrite:
	          path: ""
	          host: "rewritten.internal"
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 3. Start Pavis
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config.pvs"
run_pavis "$TEST_TMP/config.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# 4. Assert Rewrites
echo "--- Testing Path and Host Rewrites ---"
# Path /service-a/echo should be rewritten to /echo
# Host original.com should be rewritten to rewritten.internal
response=$(curl -s -H "Host: original.com" "http://127.0.0.1:$PORT_PAVIS/service-a/echo?q=bar")

# Check Rewritten Path
# Note: The mock backend /echo returns the path it received
val=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['path'])")
assert_eq "/echo" "$val" "Path should be rewritten from /service-a/echo to /echo"

# Check Rewritten Query
val=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['query'])")
assert_eq "q=bar" "$val" "Query should be preserved"

# Check Rewritten Host
val=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['headers'].get('host', [''])[0])")
assert_eq "rewritten.internal" "$val" "Host should be rewritten"

echo "✅ traffic_05_rewrite passed"
