#!/bin/bash
set -e

# e2e/suites/relay/cases/15_artifact_size_limits.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8298
CASE_TMP=$(ensure_tmp_dir "relay_15")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Config with very small size limit
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-size-limit
http:
  bind: "127.0.0.1:$PORT_RELAY"
storage:
  root_dir: "$CASE_TMP/relay_storage"
artifact:
  lkg_path: "$CASE_TMP/relay_storage/lkg.pvs"
  max_pvs_bytes: 10
pipeline:
  ingest:
    source:
      kind: file
      path: "$CASE_TMP/relay_input.yaml"
EOFCONFIG

# 2. Start Relay
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 3. Get initial version
v_start=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 4. Write valid but large config (will exceed 10 byte limit after compilation)
cat <<EOFYAML > "$CASE_TMP/relay_input.yaml"
listeners:
  - name: "default"
    address: "127.0.0.1:8081"
upstreams: []
routes: []
EOFYAML
sleep 1.5

# 5. Verify version did NOT increment (config rejected due to size)
v_after=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
if [ "$v_after" -eq "$v_start" ]; then
    echo "✅ Case 15_artifact_size_limits passed"
else
    echo "❌ Version should not have changed (size limit exceeded)"
    exit 1
fi
