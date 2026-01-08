#!/bin/bash
set -e

# Case 01: Ingest (Publish Success)
# Verifies that Relay ingests config updates via File Source.

source "$(dirname "$0")/../../lib/harness.sh"
source "$(dirname "$0")/../../lib/network.sh"
source "$(dirname "$0")/../../lib/deploy.sh"
source "$(dirname "$0")/../../lib/assert.sh"

setup_test "relay_01"
cleanup_trap() { cleanup_test; }
trap cleanup_trap EXIT

PORT_RELAY=$(get_free_port)

# 1. Prepare Relay Config
mkdir -p "$TEST_TMP/storage"
cat <<EOF > "$TEST_TMP/relay.yaml"
identity:
  name: relay-01
http:
  bind: "127.0.0.1:$PORT_RELAY"
storage:
  root_dir: "$TEST_TMP/storage"
artifact:
  lkg_path: "$TEST_TMP/storage/lkg.pvs"
pipeline:
  ingest:
    source:
      kind: file
      path: "$TEST_TMP/ingest.yaml"
EOF

touch "$TEST_TMP/ingest.yaml"

# 2. Start Relay
run_relay "$TEST_TMP/relay.yaml"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 5

# 3. Get Initial Version
STATUS=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status")
echo "DEBUG: Initial Status: $STATUS"
V_START=$(echo "$STATUS" | grep -o "version=[0-9]*" | cut -d= -f2)

if [ -z "$V_START" ]; then
    echo "❌ Failed to get initial version"
    exit 1
fi

# 4. Write Config V1 (File Ingest)
cat <<EOF > "$TEST_TMP/ingest.yaml"
listeners:
  - name: "test"
    address: "127.0.0.1:0"
upstreams: []
routes: []
EOF

# 5. Wait for Version Increment
for i in {1..20}; do
    STATUS=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status")
    V_NOW=$(echo "$STATUS" | grep -o "version=[0-9]*" | cut -d= -f2)
    
    if [ -n "$V_NOW" ] && [ "$V_NOW" -gt "$V_START" ]; then
        echo "✅ Version incremented to $V_NOW"
        exit 0
    fi
    sleep 0.2
done

echo "❌ Version did not increment"
exit 1
