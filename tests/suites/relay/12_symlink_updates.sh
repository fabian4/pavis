#!/bin/bash
set -e

# Case 12: Symlink Updates
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_12"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

mkdir -p "$TEST_TMP/data"
echo "listeners: []" > "$TEST_TMP/data/v1.yaml"
echo "listeners: []" > "$TEST_TMP/data/v2.yaml"
ln -s "$TEST_TMP/data/v1.yaml" "$TEST_TMP/link.yaml"

cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: relay-12 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/link.yaml" } } }
EOF

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

V_START=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)

ln -sf "$TEST_TMP/data/v2.yaml" "$TEST_TMP/link.yaml"
sleep 4

V_END=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V_END" -le "$V_START" ]; then echo "❌ Version not incremented"; exit 1; fi

echo "✅ Case 12_symlink_updates passed"
