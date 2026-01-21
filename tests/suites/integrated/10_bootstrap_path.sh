#!/bin/bash
set -e

# Case: smoke_01_full_path_bootstrap
# Category: Smoke & Bootstrap
# Invariants: I1 (End-to-End Publish), I2 (Hot Reload Pipeline)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "smoke_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Relay
# Must provide valid lkg_path even for memory storage
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

# 2. Start Runtime (Bootstrap)
# Initial config has listener but no routes.
cat <<-EOF > "$TEST_TMP/bootstrap.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams: []
	routes: []
EOF
gen_pvs "$TEST_TMP/bootstrap.yaml" "$TEST_TMP/bootstrap.pvs"

# Start Pavis connected to Relay
run_pavis "$TEST_TMP/bootstrap.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Verify initial state (404 for echo)
assert_status "http://127.0.0.1:$PORT_PAVIS/echo" 404

# 3. Compile & Publish Config V1 (Routes to Upstream)
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: ${UPSTREAM_HTTP_PORT_V1}
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

# Publish to Relay (Real Relay API)
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/config_v1.pvs" > /dev/null

# 4. Assert Traffic (Wait for propagation)
# Poll until 200 OK
MAX_RETRIES=20
SUCCESS=0
for _ in $(seq 1 $MAX_RETRIES); do
    if pavis_curl_body -f "http://127.0.0.1:$PORT_PAVIS/echo" > /dev/null; then
        SUCCESS=1
        break
    fi
    sleep 0.5
done

if [ "$SUCCESS" -eq 0 ]; then
    echo "❌ Traffic did not start flowing after publish"
    exit 1
fi

echo "✅ smoke_01_full_path_bootstrap passed"