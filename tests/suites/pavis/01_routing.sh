#!/bin/bash
set -e

# Case 01: Basic Routing
# Verifies that Pavis can route requests to an upstream backend.

# Source libraries (assuming run.sh sourced them, but for standalone run:)
# We use relative paths from the script location
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

# 1. Prepare Config
cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry: {}
	upstreams:
	  - name: "backend"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8081
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix
	          path: "/"
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF

gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# 2. Run Pavis
run_pavis "$TEST_TMP/config.pvs" ""

# 3. Wait for Pavis
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

# 4. Assert
assert_body "http://127.0.0.1:$PORT_PAVIS" "backend-v1"

echo "✅ Case 01_routing passed"
