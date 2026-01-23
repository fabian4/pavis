#!/bin/bash
set -e

# Case: 70_limits_oversize
# Category: Limits
# Invariants: R7 (Backpressure/Limits)

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"

setup_test "70_limits_oversize"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

# 1. Start Relay with small size limit (100 bytes)
cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	pipeline:
	  ingest:
	    source:
	      kind: none
	artifact:
	  limits:
	    max_pvs_bytes: 100
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 2. Generate a normal PVS (expected to be > 100 bytes)
gen_minimal_pvs "$TEST_TMP/valid.pvs" "test"
SIZE=$(stat -f%z "$TEST_TMP/valid.pvs" 2>/dev/null || stat -c%s "$TEST_TMP/valid.pvs")

if [ "$SIZE" -le 100 ]; then
    echo "⚠️ Minimal PVS is too small ($SIZE bytes), adding more data..."
    cat <<-EOF > "$TEST_TMP/large.yaml"
	listeners:
	  - name: "listener-large"
	    address: "127.0.0.1:0"
	upstreams:
	  - name: "large-upstream-to-increase-artifact-size-beyond-the-limit"
	    endpoints: []
	routes: []
	telemetry:
	  service_name: "large-test"
EOF
    "$PAVCTL_BIN" gen "$TEST_TMP/large.yaml" "$TEST_TMP/valid.pvs"
fi

# 3. Attempt Publish
pavis_curl_headers "$TEST_TMP/resp" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    --data-binary "@$TEST_TMP/valid.pvs"

# 4. Assert 413 Payload Too Large
assert_status_eq "$TEST_TMP/resp" 413

echo "✅ 70_limits_oversize passed"
