#!/bin/bash
set -e

# Case: reload_02_idempotent_update
# Category: End-to-End Reload
# Invariants: I2 (Hot Reload Pipeline)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "reload_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

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
	      - matcher: !prefix { path: "/" }
	        destinations:
	          - upstream: "backend"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config.yaml" "$TEST_TMP/config.pvs"

# Publish V1 (ver 1)
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/config.pvs" > /dev/null

cp "$TEST_TMP/config.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Assert Traffic
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1" \
  -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
  -H "X-Pavis-Test-Case: ${CASE_NAME}"

# Publish V1 again (ver 2, same content)
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 2" \
    --data-binary "@$TEST_TMP/config.pvs" > /dev/null

# Wait & Assert Stability
# We loop requests to ensure no drops
for i in {1..20}; do
    response=$(curl -s "http://127.0.0.1:$PORT_PAVIS/echo" \
      -H "X-Pavis-Test-Run: ${RUN_ID:-manual}" \
      -H "X-Pavis-Test-Case: ${CASE_NAME}")
    if [[ "$response" != *"backend-v1"* ]]; then
        echo "❌ Traffic failure during idempotent update"
        exit 1
    fi
    sleep 0.1
done

echo "✅ reload_02_idempotent_update passed"