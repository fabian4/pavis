#!/bin/bash
set -e

# Case: 60_resilience_restart
# Category: Resilience
# Invariants: I2, I4

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "60_resilience_restart"
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
	upstreams: [{ name: "backend", endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }] }]
	routes: [{ host: "*", paths: [{ matcher: { path: !prefix { path: "/" } }, destinations: [{ upstream: "backend", weight: 1 }] }] }]
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" --data-binary "@$TEST_TMP/config_v1.pvs" > /dev/null

# Wait for V1 to be active
MAX_RETRIES=50
for _ in $(seq 1 $MAX_RETRIES); do
    if pavis_curl_body -f "http://127.0.0.1:$PORT_PAVIS/echo" > /dev/null; then break; fi
    sleep 0.1
done
assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"

# 2. Kill Relay
stop_sut "relay"

# 3. Assert Traffic Continues (LKG)
# We send a few requests to be sure
for _ in {1..5}; do
    assert_body "http://127.0.0.1:$PORT_PAVIS/echo" "backend-v1"
    sleep 0.1
done

# 4. Restart Relay
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 5. Publish V2
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams: [{ name: "backend", endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V2} }] }]
	routes: [{ host: "*", paths: [{ matcher: { path: !prefix { path: "/" } }, destinations: [{ upstream: "backend", weight: 1 }] }] }]
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
curl -s -f -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" --data-binary "@$TEST_TMP/config_v2.pvs" > /dev/null

# 6. Assert Runtime picks up V2
MAX_RETRIES=50
SWITCHED=0
for _ in $(seq 1 $MAX_RETRIES); do
    if pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo" | grep -q "backend-v2"; then
        SWITCHED=1
        break
    fi
    sleep 0.2
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Runtime did not pick up V2 after Relay restart"
    exit 1
fi

echo "✅ 60_resilience_restart passed"
