#!/bin/bash
set -e

# Case 02: Reject Invalid PVS
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_02"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: relay-02 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF
touch "$TEST_TMP/ingest.yaml"

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

V_START=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)

CODE=$(curl -s -w "% {http_code}" -o /dev/null -X POST "http://127.0.0.1:$PORT_RELAY/v1/publish" \
    -H "X-Pavis-Version: 100" --data "garbage")

if [ "$CODE" -lt 400 ]; then echo "❌ Expected error, got $CODE"; exit 1; fi

V_END=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
if [ "$V_END" != "$V_START" ]; then echo "❌ Version changed"; exit 1; fi

echo "✅ Case 02_reject_invalid_pvs passed"
