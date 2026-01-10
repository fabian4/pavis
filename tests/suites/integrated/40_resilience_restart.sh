#!/bin/bash
set -e

# Case: 40_resilience_restart
# Category: Resilience
# Invariants: I2, I4

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "resilience_restart"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Full Path
# Relay
cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	source:
	  type: none
	artifact:
	  lkg_path: "$TEST_TMP/lkg.pvs"
EOF
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# Runtime (Bootstrap)
cat <<-EOF > "$TEST_TMP/bootstrap.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams: []
	routes: []
	telemetry: { service_name: "bootstrap" }
EOF
gen_pvs "$TEST_TMP/bootstrap.yaml" "$TEST_TMP/bootstrap.pvs"
run_pavis "$TEST_TMP/bootstrap.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Publish V1
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams: [{ name: "backend", endpoints: [{ ip: "127.0.0.1", port: 8081 }] }]
	routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: "backend", weight: 1 }] }] }]
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" -H "x-pavis-version: 1" --data-binary "@$TEST_TMP/config_v1.pvs" > /dev/null

# Wait for V1 to be active
MAX_RETRIES=50
for i in $(seq 1 $MAX_RETRIES); do
    if curl -s -f "http://127.0.0.1:$PORT_PAVIS/echo" > /dev/null; then break; fi
    sleep 0.1
done
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1" -H "X-Pavis-Test-Case: ${CASE_NAME}"

# 2. Kill Relay
stop_sut "relay"

# 3. Assert Traffic Continues (LKG)
# We send a few requests to be sure
for i in {1..5}; do
    assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1" -H "X-Pavis-Test-Case: ${CASE_NAME}"
    sleep 0.1
done

# 4. Restart Relay
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 5. Publish V2
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams: [{ name: "backend", endpoints: [{ ip: "127.0.0.1", port: 8082 }] }]
	routes: [{ host: "*", paths: [{ matcher: !prefix { path: "/" }, destinations: [{ upstream: "backend", weight: 1 }] }] }]
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" -H "x-pavis-version: 2" --data-binary "@$TEST_TMP/config_v2.pvs" > /dev/null

# 6. Assert Runtime picks up V2
MAX_RETRIES=50
SWITCHED=0
for i in $(seq 1 $MAX_RETRIES); do
    if curl -s "http://127.0.0.1:$PORT_PAVIS/echo" | grep -q "backend-v2"; then
        SWITCHED=1
        break
    fi
    sleep 0.2
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Runtime did not pick up V2 after Relay restart"
    exit 1
fi

echo "✅ 40_resilience_restart passed"