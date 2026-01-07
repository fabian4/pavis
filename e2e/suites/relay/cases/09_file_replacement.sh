#!/bin/bash
set -e

# e2e/suites/relay/cases/09_file_replacement.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8292
CASE_TMP=$(ensure_tmp_dir "relay_09")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Config
mkdir -p "$CASE_TMP/relay_storage"
echo "{}" > "$CASE_TMP/relay_input.yaml"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-file-replacement
http:
  bind: "127.0.0.1:$PORT_RELAY"
storage:
  root_dir: "$CASE_TMP/relay_storage"
artifact:
  lkg_path: "$CASE_TMP/relay_storage/lkg.pvs"
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

# 4. Simulate atomic file replacement (mv)
TMP_PATH="$CASE_TMP/relay_input.tmp"
cat <<EOFYAML > "$TMP_PATH"
listeners:
  - name: "replaced"
    address: "127.0.0.1:8081"
upstreams: []
routes: []
EOFYAML
mv "$TMP_PATH" "$CASE_TMP/relay_input.yaml"

# 5. Wait for debounce and processing
sleep 1.5

# 6. Verify version incremented
v_after=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
if [ "$v_after" -gt "$v_start" ]; then
    echo "✅ Case 09_file_replacement passed"
else
    echo "❌ Version did not increment: start=$v_start, after=$v_after"
    exit 1
fi
