#!/bin/bash
set -e

# Case 10: Startup Corrupted LKG
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_10"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

mkdir -p "$TEST_TMP/storage"
echo "CORRUPT" > "$TEST_TMP/storage/lkg.pvs"

cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: relay-10 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline: { ingest: { source: { kind: file, path: "$TEST_TMP/ingest.yaml" } } }
EOF
touch "$TEST_TMP/ingest.yaml"

run_relay "$TEST_TMP/relay.yaml"
sleep 2

if [ "$TEST_MODE" == "binary" ]; then
    PID=$(cat "$TEST_TMP/pids/relay.pid")
    if kill -0 "$PID" 2>/dev/null; then echo "❌ Relay running"; exit 1; fi
else
    CID=$(cat "$TEST_TMP/pids/relay.container")
    if docker ps -q --no-trunc | grep -q "$CID"; then echo "❌ Relay running"; exit 1; fi
fi

echo "✅ Case 10_startup_corrupted_lkg passed"
