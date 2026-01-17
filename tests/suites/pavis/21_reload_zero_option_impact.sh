#!/bin/bash
set -e

# Case: lifecycle_21_reload_zero_option
# Category: Reload Semantics
# Invariants: D (Zero-Option)
# Description: Verify that fields removed from the config artifact are immediately removed from runtime behavior, proving no hidden defaults or state carry-over.

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "lifecycle_21"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# V1: Has a custom response header
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams: [{ name: "backend", endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }] }]
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        response_headers:
	          set_headers: [["X-Pavis-Version", "v1"]]
	        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# Verify V1 header presence
if ! curl -sI "http://127.0.0.1:$PORT_PAVIS/echo" | grep -qi "X-Pavis-Version: v1"; then
    echo "❌ V1 header missing"
    exit 1
fi

# V2: Removed the header policy entirely
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners: [{ name: "default", address: "127.0.0.1:$PORT_PAVIS" }]
	upstreams: [{ name: "backend", endpoints: [{ ip: "127.0.0.1", port: ${UPSTREAM_HTTP_PORT_V1} }] }]
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix { path: "/" }
	        destinations: [{ upstream: "backend", weight: 1 }]
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2.pvs"
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2.pvs"

# Wait for switch (poll for header absence)
MAX_RETRIES=20
SWITCHED=0
for _ in $(seq 1 $MAX_RETRIES); do
    if ! curl -sI "http://127.0.0.1:$PORT_PAVIS/echo" | grep -qi "X-Pavis-Version"; then
        SWITCHED=1
        break
    fi
    sleep 0.5
done

if [ "$SWITCHED" -eq 0 ]; then
    echo "❌ Invariant D violated: Removed header 'X-Pavis-Version' still present after reload"
    exit 1
fi

echo "✅ lifecycle_21_reload_zero_option passed"
