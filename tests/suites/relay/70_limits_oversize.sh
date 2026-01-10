#!/bin/bash
set -e

# Case: 70_limits_oversize
# Category: Limits
# Invariants: R7 (Backpressure/Limits)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "limits_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

# 1. Start Relay with small size limit (100 bytes)
cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
	source:
	  type: none
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
	listeners: []
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
CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/valid.pvs")

# 4. Assert 413 Payload Too Large
if [ "$CODE" != "413" ]; then
    echo "❌ Expected 413 Payload Too Large, got $CODE"
    exit 1
fi

echo "✅ 70_limits_oversize passed"
