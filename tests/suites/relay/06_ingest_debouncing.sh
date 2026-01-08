#!/bin/bash
set -e

# Case 06: Ingest Debouncing
source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_06"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

cat <<EOF > "$TEST_TMP/relay.yaml"
identity: { name: relay-06 }
http: { bind: "127.0.0.1:$PORT_RELAY" }
storage: { root_dir: "$TEST_TMP/storage" }
artifact: { lkg_path: "$TEST_TMP/storage/lkg.pvs" }
pipeline:
  ingest:
    source: { kind: file, path: "$TEST_TMP/ingest.yaml" }
    mode: { debounce: 500 }
EOF
touch "$TEST_TMP/ingest.yaml"

run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

V_START=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)

for i in {1..5}; do
    echo "listeners: []" > "$TEST_TMP/ingest.yaml"
    sleep 0.1
done

sleep 1
V_END=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o "version=[0-9]*" | cut -d= -f2)
EXPECTED=$((V_START + 1))

if [ "$V_END" != "$EXPECTED" ]; then echo "❌ Debouncing failed. Expected $EXPECTED, got $V_END"; exit 1; fi

echo "✅ Case 06_ingest_debouncing passed"