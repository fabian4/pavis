#!/bin/bash
set -e

# Case: 10_contract_opaque
# Category: Contract & Integrity
# Invariants: R1 (Opaque), R2 (ETag), A5 (Relay Opacity)
#
# This test verifies that:
# 1. The relay is opaque: it stores and returns bytes exactly as received.
# 2. The runtime rejects artifacts that have been tampered with (checksum mismatch).

# shellcheck source=tests/scripts/env.sh
source "$(dirname "$0")/../../scripts/env.sh"
# shellcheck source=tests/scripts/assert.sh
source "$(dirname "$0")/../../scripts/assert.sh"
# shellcheck source=tests/scripts/wait_helpers.sh
source "$(dirname "$0")/../../scripts/wait_helpers.sh"

setup_test "10_contract_opaque"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)
PORT_PAVIS=$(get_free_port)
PORT_METRICS=$(get_free_port)

# Use mock relay for this test as it allows publishing tampered bytes
# (the real relay binary now validates integrity on publish).
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# --- Step 1: Verify Opacity (Blind Bit Pipe) ---
echo "Verifying relay opacity (blind bit pipe)..."

# Configuration with a dummy route to ensure /healthz works
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	telemetry:
	  metrics: "127.0.0.1:$PORT_METRICS"
	upstreams:
	  - name: "dummy"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 1
	routes:
	  - host: "*"
	    paths:
	      - matcher:
	          path: !prefix { path: "/healthz" }
	        destinations:
	          - upstream: "dummy"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/valid_v1.pvs"

# Publish v1
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/valid_v1.pvs"

# Subscribe and verify bytes
fetch_with_headers "http://127.0.0.1:$PORT_RELAY/v1/config" \
    "$TEST_TMP/sub_resp" "$TEST_TMP/body"

assert_status_eq "$TEST_TMP/sub_resp" 200

if ! cmp -s "$TEST_TMP/valid_v1.pvs" "$TEST_TMP/body"; then
    fail "Relay body mismatch - failed opacity requirement"
fi

# --- Step 2: Verify Integrity Rejection (Tampering Detection) ---
echo "Verifying runtime integrity rejection (tampering detection)..."

# Start pavis with valid v1
run_pavis "$TEST_TMP/valid_v1.pvs" "http://127.0.0.1:$PORT_RELAY"
# We expect 200/404 for healthz, wait_for_url should succeed if it responds
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5
wait_for_port "$PORT_METRICS" 5

# Create tampered v2
# Flip a bit in the payload (offset 128)
cp "$TEST_TMP/valid_v1.pvs" "$TEST_TMP/tampered_v2.pvs"
printf 'X' | dd of="$TEST_TMP/tampered_v2.pvs" bs=1 seek=128 count=1 conv=notrunc 2>/dev/null

echo "Publishing tampered v2 artifact..."
# Mock relay accepts anything
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/tampered_v2.pvs"

# Runtime should poll and reject
echo "Waiting for runtime to detect tampering..."
if ! wait_for_log "Checksum mismatch" "$TEST_TMP/logs/pavis.log" 15; then
    fail "Runtime did not log checksum mismatch for tampered artifact"
fi

# Verify runtime is still on version 1
VERSION=$(get_runtime_config_version "http://127.0.0.1:$PORT_METRICS/metrics")
# version might be empty if not updated yet, but we want to ensure it's NOT 2.
# rev 1 -> v1. rev 2 -> tampered v2.
# Since v2 is rejected, metrics should still say v1.
if [ "$VERSION" != "1" ]; then
    fail "Runtime applied tampered configuration! (Version reached $VERSION)"
fi

echo "✅ contract_01_opaque_publish_subscribe passed"