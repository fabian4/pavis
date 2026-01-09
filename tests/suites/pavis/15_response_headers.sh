#!/bin/bash
set -e

# Case 15: Response Headers
source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "pavis_15"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)

# Using shared upstream backend-v1 (8081) which returns JSON with Content-Type: application/json
cat <<-EOF > "$TEST_TMP/config.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend"
	    endpoints:
	      - address: "127.0.0.1"
	        port: 8081
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix
	          path: "/"
	        response_headers:
	          add_headers:
	            - ["X-Add", "Yes"]
	          remove_headers:
	            - "Content-Type"
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

run_pavis "$TEST_TMP/config.pvs" ""
wait_for_url "http://127.0.0.1:$PORT_PAVIS" 5

RESP=$(curl -s -i "http://127.0.0.1:$PORT_PAVIS")
if ! echo "$RESP" | grep -qi "X-Add: Yes"; then echo "❌ Added header missing"; exit 1; fi
if echo "$RESP" | grep -qi "Content-Type: application/json"; then echo "❌ Removed header present"; exit 1; fi

echo "✅ Case 15_response_headers passed"
