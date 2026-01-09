#!/bin/bash
set -e

# Case: security_01_tls_origination_toggle
# Category: Security & TLS
# Invariants: C (Atomic Switch)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "security_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V1: HTTP (Port 8081)
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
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
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert V1 (HTTP)
response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
  -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
  -H "X-Pavis-Test-Case: ${CASE_NAME}")
tls_enabled=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['tls']['enabled'])")
if [ "$tls_enabled" == "True" ] || [ "$tls_enabled" == "true" ]; then
    echo "❌ Expected HTTP initially, got TLS enabled"
    exit 1
fi

# V2: HTTPS (Port 8443)
# Note: verify_cert: false is required for mock upstream
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend"
	    tls:
	      enabled: true
	      verify_cert: false
	      verify_hostname: false
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8443
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"

publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Wait for switch (poll for TLS enabled)
MAX_RETRIES=20
SWITCHED=0
for i in $(seq 1 $MAX_RETRIES); do
    response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
      -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
      -H "X-Pavis-Test-Case: ${CASE_NAME}")
    
    tls_enabled=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin).get('tls', {}).get('enabled', False))")
    
    if [ "$tls_enabled" == "True" ] || [ "$tls_enabled" == "true" ]; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Traffic did not switch to HTTPS (TLS) after reload"
    exit 1
fi

echo "✅ security_01_tls_origination_toggle passed"
