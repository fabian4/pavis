#!/bin/bash
set -e

# e2e/suites/relay/cases/12_symlink_updates.sh

source "$E2E_ROOT/scripts/lib/process.sh"
source "$E2E_ROOT/scripts/lib/http.sh"
source "$E2E_ROOT/scripts/lib/fs.sh"

PORT_RELAY=8295
CASE_TMP=$(ensure_tmp_dir "relay_12")

cleanup() {
    stop_pid "$CASE_TMP/relay.pid"
}
trap cleanup EXIT

# 1. Setup data directory with config files
DATA_DIR="$CASE_TMP/data"
mkdir -p "$DATA_DIR"

V1_PATH="$DATA_DIR/v1.yaml"
V2_PATH="$DATA_DIR/v2.yaml"
LINK_PATH="$CASE_TMP/config.yaml"

cat <<EOFYAML > "$V1_PATH"
listeners: []
upstreams: []
routes: []
EOFYAML

cat <<EOFYAML > "$V2_PATH"
listeners:
  - name: "v2"
    address: "127.0.0.1:8080"
upstreams: []
routes: []
EOFYAML

# Create initial symlink
ln -s "$V1_PATH" "$LINK_PATH"

# 2. Config
mkdir -p "$CASE_TMP/relay_storage"

RELAY_CONFIG="$CASE_TMP/relay_config.yaml"
cat <<EOFCONFIG > "$RELAY_CONFIG"
identity:
  name: pavis-relay-symlink
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
      path: "$LINK_PATH"
EOFCONFIG

# 3. Start Relay
RUST_LOG=info "$RELAY_BIN" --config "$RELAY_CONFIG" > "$CASE_TMP/relay.log" 2>&1 &
echo $! > "$CASE_TMP/relay.pid"
wait_for_url "http://127.0.0.1:$PORT_RELAY/health" 10

# 4. Get initial version
v_start=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)

# 5. Update symlink atomically (typical K8s ConfigMap behavior)
TMP_LINK="$CASE_TMP/config.tmp"
ln -s "$V2_PATH" "$TMP_LINK"
mv -f "$TMP_LINK" "$LINK_PATH"

# 6. Wait for either notify or polling fallback (2s poll + 500ms debounce + buffer)
sleep 4

# 7. Verify version incremented
v_after=$(curl -s "http://127.0.0.1:$PORT_RELAY/v1/status" | grep -o '"version":[0-9]*' | cut -d: -f2)
if [ "$v_after" -gt "$v_start" ]; then
    echo "✅ Case 12_symlink_updates passed"
else
    echo "❌ Version should have incremented after symlink update"
    exit 1
fi
