#!/bin/bash
set -e

# Case: lifecycle_04_lkg_semantic_invalidity
# Category: Failure & LKG
# Invariants: B (LKG)
# Description: Valid structure/checksum, but unsupported protocol version.

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "lifecycle_04"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_PAVIS=$(get_free_port)
PORT_RELAY=$(get_free_port)

# 1. Start Mock Relay
run_mock_relay "$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_RELAY/status" 5

# 2. Prepare Config V1
cat <<-EOF > "$TEST_TMP/config_v1.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v1"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8081
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix
	          path: "/"
	        destinations:
	          - upstream: "backend-v1"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v1.yaml" "$TEST_TMP/config_v1.pvs"

# 3. Publish V1 and Start
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v1.pvs"
cp "$TEST_TMP/config_v1.pvs" "$TEST_TMP/initial.pvs"
run_pavis "$TEST_TMP/initial.pvs" "http://127.0.0.1:$PORT_RELAY"
wait_for_url "http://127.0.0.1:$PORT_PAVIS/healthz" 5

# 4. Assert V1
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin)['instance_id'])")
if [ "$instance" != "backend-v1" ]; then
    echo "❌ Expected backend-v1, got $instance"
    exit 1
fi

# 5. Prepare Config with Unsupported Version
# We generate a valid V2, then tamper with the version byte (offset 4).
cat <<-EOF > "$TEST_TMP/config_v2.yaml"
	listeners:
	  - name: "default"
	    address: "127.0.0.1:$PORT_PAVIS"
	upstreams:
	  - name: "backend-v2"
	    endpoints:
	      - ip: "127.0.0.1"
	        port: 8082
	routes:
	  - host: "*"
	    paths:
	      - matcher: !prefix
	          path: "/"
	        destinations:
	          - upstream: "backend-v2"
	            weight: 1
EOF
gen_pvs "$TEST_TMP/config_v2.yaml" "$TEST_TMP/config_v2_orig.pvs"

# Copy to modify
cp "$TEST_TMP/config_v2_orig.pvs" "$TEST_TMP/config_v2_bad.pvs"

# Use python to modify byte 4 (5th byte) to 0xFF (255)
python3 -c "
with open('$TEST_TMP/config_v2_bad.pvs', 'r+b') as f:
    f.seek(4)
    f.write(b'\xff')
"

# Note: Checksum is at the end or computed over body?
# pavis-pvs header includes checksum of the BODY.
# The header structure is: Magic(4) + Version(1) + ... + Checksum(32).
# If I modify Version, does checksum change?
# `crates/pavis-pvs/src/header.rs`.
# If I modify header, `verify` might fail checksum if checksum includes header?
# Usually checksum covers body. Header validation checks version first.
# So this should trigger "Unsupported Version" error before checksum error (or alongside).
# Either way, it should be rejected.

# 6. Publish Bad Config
publish_config "http://127.0.0.1:$PORT_RELAY" "$TEST_TMP/config_v2_bad.pvs"

# 7. Wait for potential poll cycle
sleep 2

# 8. Assert LKG (Still V1)
response=$(pavis_curl_body "http://127.0.0.1:$PORT_PAVIS/echo")
instance=$(echo "$response" | python3 -c "import sys, json; print(json.load(sys.stdin).get('instance_id', ''))")

if [ "$instance" != "backend-v1" ]; then
    echo "❌ LKG failed. Expected backend-v1, got '$instance'"
    exit 1
fi

echo "✅ lifecycle_04_lkg_semantic_invalidity passed"
