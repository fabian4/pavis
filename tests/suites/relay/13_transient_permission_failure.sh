#!/bin/bash
set -e

# Case 13: Transient Permission Failure
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_13"
cleanup_trap() { chmod 644 "$TEST_TMP/ingest.yaml" 2>/dev/null || true; cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: relay-13 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF
touch "$TEST_TMP/ingest.yaml"

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

V_START=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)

chmod 000 "$TEST_TMP/ingest.yaml"
sleep 1
chmod 644 "$TEST_TMP/ingest.yaml"
echo "listeners: []" > "$TEST_TMP/ingest.yaml"
sleep 2

V_END=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V_END" -le "$V_START" ]; then echo "❌ Version not incremented"; exit 1; fi

echo "✅ Case 13_transient_permission_failure passed"
