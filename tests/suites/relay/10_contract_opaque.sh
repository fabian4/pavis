#!/bin/bash
set -e

# Case: contract_01_opaque_publish_subscribe
# Category: Contract & Integrity
# Invariants: R1 (Opaque), R2 (Versioned)

source "$(dirname "$0")/../../lib/env.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "contract_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

# 1. Start Relay (Real)
cat <<-EOF > "$TEST_TMP/relay.yaml"
	http:
	  bind: "127.0.0.1:$PORT_RELAY"
	storage:
	  type: memory
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 2. Generate Valid PVS
gen_minimal_pvs "$TEST_TMP/valid.pvs" "test1"

# 3. Publish
response=$(curl -s -i -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "x-pavis-version: 1" \
    --data-binary "@$TEST_TMP/valid.pvs")

if ! echo "$response" | grep -q "200 OK"; then
    echo "❌ Publish failed"
    echo "$response"
    exit 1
fi

# 4. Subscribe
response=$(curl -s -i "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 0")

# 5. Assertions
if ! echo "$response" | grep -q "200 OK"; then
    echo "❌ Subscribe failed"
    echo "$response"
    exit 1
fi

# Check Body
curl -s "http://127.0.0.1:$PORT_RELAY/v1/config" -H "x-pavis-version: 0" > "$TEST_TMP/body"
if ! cmp -s "$TEST_TMP/valid.pvs" "$TEST_TMP/body"; then
    echo "❌ Body mismatch"
    ls -l "$TEST_TMP/valid.pvs" "$TEST_TMP/body"
    exit 1
fi

# Check ETag / Version
version=$(echo "$response" | grep -i "x-pavis-version:" | awk '{print $2}' | tr -d '\r')
if [ "$version" != "1" ]; then
    echo "❌ Version mismatch. Expected 1, got '$version'"
    exit 1
fi

echo "✅ contract_01_opaque_publish_subscribe passed"