#!/bin/bash
set -e

# Case 11: Rapid Toggle
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_11"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: relay-11 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" }, debounce_ms: 100 } }
EOF
touch "$TEST_TMP/ingest.yaml"

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

V_START=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)

echo "listeners: []" > "$TEST_TMP/ingest.yaml"
sleep 0.5
V1=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V1" -le "$V_START" ]; then echo "❌ Valid update failed"; exit 1; fi

echo "listeners: [" > "$TEST_TMP/ingest.yaml"
sleep 0.5
V2=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V2" -ne "$V1" ]; then echo "❌ Invalid update changed version"; exit 1; fi

echo "listeners: []" > "$TEST_TMP/ingest.yaml"
sleep 0.5
V3=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V3" -le "$V2" ]; then echo "❌ Recovery failed"; exit 1; fi

echo "✅ Case 11_rapid_toggle passed"
